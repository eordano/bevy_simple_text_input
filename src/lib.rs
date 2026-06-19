//! A Bevy plugin that provides a simple single-line text input widget.
//!
//! # Bevy 0.19 / Parley note
//!
//! Bevy 0.19 replaced its text engine (Cosmic Text) with Parley and grew a
//! *native* editable-text widget ([`bevy::text::EditableText`], driven by a
//! [`parley::PlainEditor`]). The cosmic-text based editing/cursor/selection
//! engine this crate used through 0.18 no longer has a backing buffer to talk
//! to: `ComputedTextBlock::buffer()` is now a `parley::Layout`, and
//! `CosmicBuffer`/`CosmicFontSystem`/`Editor`/`Action`/`Change`/`Selection`
//! were all removed from `bevy_text`.
//!
//! This branch therefore re-implements the crate's internals on top of Bevy's
//! own [`EditableText`]/[`PlainEditor`], while preserving the public API
//! ([`TextInput`], [`TextInputValue`], [`TextInputSubmitEvent`], etc.) that
//! downstream consumers depend on. See `PORT-NOTES.md` for the full rationale
//! and the list of behaviours that changed.
//!
//! # Examples
//!
//! See the [examples](https://github.com/rparrett/bevy_simple_text_input/tree/latest/examples) folder.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_simple_text_input::{TextInput, TextInputPlugin};
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(TextInputPlugin)
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2d);
//!     commands.spawn((
//!         TextInput,
//!         Node {
//!             padding: UiRect::all(Val::Px(5.0)),
//!             border: UiRect::all(Val::Px(2.0)),
//!             ..default()
//!         },
//!         BorderColor::all(Color::BLACK),
//!     ));
//! }
//! ```

use bevy::{
    ecs::message::MessageCursor,
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
    text::{EditableText, TextCursorStyle, TextEdit},
};

/// A Bevy `Plugin` providing the systems and assets required to make a [`TextInput`] work.
pub struct TextInputPlugin;

/// Label for systems that update text inputs.
#[derive(Debug, PartialEq, Eq, Clone, Hash, SystemSet)]
pub struct TextInputSystem;

impl Plugin for TextInputPlugin {
    fn build(&self, app: &mut App) {
        // `EditableText`'s rendering requires `TextScroll`, `TextNodeFlags` and
        // `ContentSize` on the same entity. Bevy only registers these as required
        // components inside `bevy_ui_widgets::EditableTextInputPlugin`, which we
        // deliberately do *not* add (it would install Bevy's own keyboard / pointer
        // / focus handling, conflicting with this crate's custom input handling).
        // Register them ourselves so the native text layout/render path matches our
        // `EditableText` entities even with a plain `DefaultPlugins` app.
        use bevy::ui::{widget::TextNodeFlags, widget::TextScroll, ContentSize};
        app.register_required_components::<EditableText, Node>()
            .register_required_components::<EditableText, TextNodeFlags>()
            .register_required_components::<EditableText, ContentSize>()
            .register_required_components::<EditableText, TextScroll>();

        app.init_resource::<TextInputNavigationBindings>()
            .add_message::<TextInputSubmitEvent>()
            .add_message::<TextInputPointerEvent>()
            .add_observer(create)
            .add_systems(
                Update,
                (
                    sync_settings,
                    sync_style,
                    sync_inactive,
                    show_hide_placeholder,
                    keyboard,
                    pointer,
                    update_value,
                )
                    .chain()
                    .in_set(TextInputSystem),
            )
            .register_type::<TextInputSettings>()
            .register_type::<TextInputTextFont>()
            .register_type::<TextInputTextColor>()
            .register_type::<TextInputSelectionStyle>()
            .register_type::<TextInputInactive>()
            .register_type::<TextInputCursorTimer>()
            .register_type::<TextInputInner>()
            .register_type::<TextInputValue>()
            .register_type::<TextInputPlaceholder>();
    }
}

/// The main "driving component" for the Text Input.
///
/// # Example
///
/// ```rust
/// # use bevy::prelude::*;
/// use bevy_simple_text_input::TextInput;
/// fn setup(mut commands: Commands) {
///     commands.spawn(TextInput);
/// }
/// ```
#[derive(Component, Default)]
#[require(
    TextInputSettings,
    TextInputTextFont,
    TextInputTextColor,
    TextInputSelectionStyle,
    TextInputInactive,
    TextInputCursorTimer,
    TextInputValue,
    TextInputPlaceholder,
    Node,
    Interaction
)]
pub struct TextInput;

/// The Bevy `TextFont` that will be used when creating the text input's inner text.
#[derive(Component, Default, Reflect)]
pub struct TextInputTextFont(pub TextFont);

/// The Bevy `TextColor` that will be used when creating the text input's inner text.
#[derive(Component, Default, Reflect)]
pub struct TextInputTextColor(pub TextColor);

/// selection color and background color
#[derive(Component, Default, Reflect)]
pub struct TextInputSelectionStyle {
    /// selected text color
    pub color: Option<Color>,
    /// selected text background color
    pub background: Option<Color>,
}

/// If true, the text input does not respond to keyboard events and the cursor is hidden.
#[derive(Component, Default, Reflect)]
pub struct TextInputInactive(pub bool);

/// A component that manages the cursor's blinking.
///
/// Retained for API compatibility. Cursor blinking itself is now handled by
/// Bevy's native [`EditableText`] rendering; this component is still accepted on
/// text input entities (the app inserts it) but is no longer wired to a custom
/// blink system.
#[derive(Component, Reflect)]
pub struct TextInputCursorTimer {
    /// The timer that blinks the cursor on and off, and resets when the user types.
    pub timer: Timer,
    should_reset: bool,
}

impl Default for TextInputCursorTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            should_reset: false,
        }
    }
}

/// A component containing the text input's settings.
#[derive(Component, Default, Reflect)]
pub struct TextInputSettings {
    /// multiline
    pub multiline: bool,
    /// If true, text is not cleared after pressing enter.
    pub retain_on_submit: bool,
    /// Mask text with the provided character.
    ///
    /// Note: native [`EditableText`] rendering does not currently support
    /// character masking, so this setting is accepted but not yet honoured on
    /// the 0.19 (Parley) branch. See `PORT-NOTES.md`.
    pub mask_character: Option<char>,
}

/// Text navigation actions that can be bound via `TextInputNavigationBindings`.
#[derive(Debug)]
pub enum TextInputAction {
    /// Moves the cursor one char to the left.
    CharLeft,
    /// Moves the cursor one char to the right.
    CharRight,
    /// Moves the cursor to the start of line.
    LineStart,
    /// Moves the cursor to the end of line.
    LineEnd,
    /// move up one line
    LineUp,
    /// move down one line
    LineDown,
    /// document start
    TextStart,
    /// document end
    TextEnd,
    /// Moves the cursor one word to the left.
    WordLeft,
    /// Moves the cursor one word to the right.
    WordRight,
    /// Removes the char left of the cursor.
    DeletePrev,
    /// Removes the char right of the cursor.
    DeleteNext,
    /// Triggers a `TextInputSubmitEvent`, optionally clearing the text input.
    Submit,
    /// add a new line
    NewLine,
    /// select full buffer
    SelectAll,
    /// cut
    Cut,
    /// copy
    Copy,
    /// pasta
    Paste,
    /// undo (no-op on the 0.19 branch; `EditableText` does not yet support undo/redo)
    Undo,
    /// redo (no-op on the 0.19 branch; `EditableText` does not yet support undo/redo)
    Redo,
}
/// A resource in which key bindings can be specified. Bindings are given as a tuple of (`TextInputAction`, `TextInputBinding`).
///
/// All modifiers must be held when the primary key is pressed to perform the action.
/// The first matching action in the list will be performed, so a binding that is the same as another with additional
/// modifier keys should be earlier in the vector to be applied.
#[derive(Resource)]
pub struct TextInputNavigationBindings(pub Vec<(TextInputAction, TextInputBinding)>);

/// A combination of a key and required modifier keys that might trigger a `TextInputAction`.
pub struct TextInputBinding {
    /// Primary key
    key: KeyCode,
    /// Required modifier keys
    modifiers: Vec<KeyCode>,
}

impl TextInputBinding {
    /// Creates a new `TextInputBinding` from a key and required modifiers.
    pub fn new(key: KeyCode, modifiers: impl Into<Vec<KeyCode>>) -> Self {
        Self {
            key,
            modifiers: modifiers.into(),
        }
    }
}

impl Default for TextInputNavigationBindings {
    fn default() -> Self {
        #[cfg(not(target_os = "macos"))]
        return Self::non_macos_default();

        #[cfg(target_os = "macos")]
        Self::macos_default()
    }
}

impl TextInputNavigationBindings {
    /// default key bindings for all except macos.
    /// usually Default::default is fine, but on wasm you need to specify manually
    pub fn non_macos_default() -> Self {
        use KeyCode::*;
        use TextInputAction::*;
        Self(vec![
            (TextStart, TextInputBinding::new(Home, [ControlLeft])),
            (TextStart, TextInputBinding::new(Home, [ControlRight])),
            (TextEnd, TextInputBinding::new(End, [ControlLeft])),
            (TextEnd, TextInputBinding::new(End, [ControlRight])),
            (LineStart, TextInputBinding::new(Home, [])),
            (LineEnd, TextInputBinding::new(End, [])),
            (WordLeft, TextInputBinding::new(ArrowLeft, [ControlLeft])),
            (WordLeft, TextInputBinding::new(ArrowLeft, [ControlRight])),
            (WordRight, TextInputBinding::new(ArrowRight, [ControlLeft])),
            (WordRight, TextInputBinding::new(ArrowRight, [ControlRight])),
            (CharLeft, TextInputBinding::new(ArrowLeft, [])),
            (CharRight, TextInputBinding::new(ArrowRight, [])),
            (LineUp, TextInputBinding::new(ArrowUp, [])),
            (LineDown, TextInputBinding::new(ArrowDown, [])),
            (DeletePrev, TextInputBinding::new(Backspace, [])),
            (DeletePrev, TextInputBinding::new(NumpadBackspace, [])),
            (DeleteNext, TextInputBinding::new(Delete, [])),
            // submit must be before newline as it is the same but with modifiers
            (Submit, TextInputBinding::new(Enter, [ShiftLeft])),
            (Submit, TextInputBinding::new(Enter, [ShiftRight])),
            (NewLine, TextInputBinding::new(Enter, [])),
            (Submit, TextInputBinding::new(NumpadEnter, [])),
            (SelectAll, TextInputBinding::new(KeyA, [ControlLeft])),
            (SelectAll, TextInputBinding::new(KeyA, [ControlRight])),
            (
                TextInputAction::Cut,
                TextInputBinding::new(KeyX, [ControlLeft]),
            ),
            (
                TextInputAction::Cut,
                TextInputBinding::new(KeyX, [ControlRight]),
            ),
            (
                TextInputAction::Copy,
                TextInputBinding::new(KeyC, [ControlLeft]),
            ),
            (
                TextInputAction::Copy,
                TextInputBinding::new(KeyC, [ControlRight]),
            ),
            (
                TextInputAction::Paste,
                TextInputBinding::new(KeyV, [ControlLeft]),
            ),
            (
                TextInputAction::Paste,
                TextInputBinding::new(KeyV, [ControlRight]),
            ),
            (
                TextInputAction::Undo,
                TextInputBinding::new(KeyZ, [ControlLeft]),
            ),
            (
                TextInputAction::Undo,
                TextInputBinding::new(KeyZ, [ControlRight]),
            ),
            (
                TextInputAction::Redo,
                TextInputBinding::new(KeyY, [ControlLeft]),
            ),
            (
                TextInputAction::Redo,
                TextInputBinding::new(KeyY, [ControlRight]),
            ),
        ])
    }

    /// default key bindings for macos
    /// usually Default::default is fine, but on wasm you need to specify manually
    pub fn macos_default() -> Self {
        use KeyCode::*;
        use TextInputAction::*;
        Self(vec![
            (TextStart, TextInputBinding::new(ArrowUp, [SuperLeft])),
            (TextStart, TextInputBinding::new(ArrowUp, [SuperRight])),
            (TextStart, TextInputBinding::new(Home, [SuperLeft])),
            (TextStart, TextInputBinding::new(Home, [SuperRight])),
            (TextEnd, TextInputBinding::new(ArrowDown, [SuperLeft])),
            (TextEnd, TextInputBinding::new(ArrowDown, [SuperRight])),
            (TextEnd, TextInputBinding::new(End, [SuperLeft])),
            (TextEnd, TextInputBinding::new(End, [SuperRight])),
            (LineStart, TextInputBinding::new(ArrowLeft, [SuperLeft])),
            (LineStart, TextInputBinding::new(ArrowLeft, [SuperRight])),
            (LineStart, TextInputBinding::new(Home, [])),
            (LineEnd, TextInputBinding::new(ArrowRight, [SuperLeft])),
            (LineEnd, TextInputBinding::new(ArrowRight, [SuperRight])),
            (LineEnd, TextInputBinding::new(End, [])),
            (WordLeft, TextInputBinding::new(ArrowLeft, [AltLeft])),
            (WordLeft, TextInputBinding::new(ArrowLeft, [AltRight])),
            (WordRight, TextInputBinding::new(ArrowRight, [AltLeft])),
            (WordRight, TextInputBinding::new(ArrowRight, [AltRight])),
            (CharLeft, TextInputBinding::new(ArrowLeft, [])),
            (CharRight, TextInputBinding::new(ArrowRight, [])),
            (LineUp, TextInputBinding::new(ArrowUp, [])),
            (LineDown, TextInputBinding::new(ArrowDown, [])),
            (DeletePrev, TextInputBinding::new(Backspace, [])),
            (DeletePrev, TextInputBinding::new(NumpadBackspace, [])),
            (DeleteNext, TextInputBinding::new(Delete, [])),
            // submit must be before newline as it is the same but with modifiers
            (Submit, TextInputBinding::new(Enter, [ShiftLeft])),
            (Submit, TextInputBinding::new(Enter, [ShiftRight])),
            (Submit, TextInputBinding::new(Enter, [AltLeft])),
            (Submit, TextInputBinding::new(Enter, [AltRight])),
            (NewLine, TextInputBinding::new(Enter, [])),
            (Submit, TextInputBinding::new(NumpadEnter, [])),
            (SelectAll, TextInputBinding::new(KeyA, [SuperLeft])),
            (SelectAll, TextInputBinding::new(KeyA, [SuperRight])),
            (
                TextInputAction::Cut,
                TextInputBinding::new(KeyX, [SuperLeft]),
            ),
            (
                TextInputAction::Cut,
                TextInputBinding::new(KeyX, [SuperRight]),
            ),
            (
                TextInputAction::Copy,
                TextInputBinding::new(KeyC, [SuperLeft]),
            ),
            (
                TextInputAction::Copy,
                TextInputBinding::new(KeyC, [SuperRight]),
            ),
            (
                TextInputAction::Paste,
                TextInputBinding::new(KeyV, [SuperLeft]),
            ),
            (
                TextInputAction::Paste,
                TextInputBinding::new(KeyV, [SuperRight]),
            ),
            // Redo (Cmd+Shift+Z) must come before Undo (Cmd+Z): the matcher
            // picks the first binding whose modifiers are all held, and Undo's
            // modifiers are a subset of Redo's.
            (
                TextInputAction::Redo,
                TextInputBinding::new(KeyZ, [SuperLeft, ShiftLeft]),
            ),
            (
                TextInputAction::Redo,
                TextInputBinding::new(KeyZ, [SuperRight, ShiftLeft]),
            ),
            (
                TextInputAction::Redo,
                TextInputBinding::new(KeyZ, [SuperLeft, ShiftRight]),
            ),
            (
                TextInputAction::Redo,
                TextInputBinding::new(KeyZ, [SuperRight, ShiftRight]),
            ),
            (
                TextInputAction::Undo,
                TextInputBinding::new(KeyZ, [SuperLeft]),
            ),
            (
                TextInputAction::Undo,
                TextInputBinding::new(KeyZ, [SuperRight]),
            ),
        ])
    }
}

/// A component containing the current value of the text input.
#[derive(Component, Default, Reflect)]
pub struct TextInputValue(pub String);

/// A component containing the placeholder text that is displayed when the text input is empty and not focused.
#[derive(Component, Default, Reflect)]
pub struct TextInputPlaceholder {
    /// The placeholder text.
    pub value: String,
    /// The `TextFont` to use when rendering the placeholder text.
    ///
    /// If `None`, the text input font will be used.
    pub text_font: Option<TextFont>,
    /// The style to use when rendering the placeholder text.
    ///
    /// If `None`, the text input color will be used with alpha value of `0.25`.
    pub text_color: Option<TextColor>,
}

#[derive(Component, Reflect)]
struct TextInputPlaceholderInner;

#[derive(Component, Reflect)]
struct TextInputInner;

/// An event that is fired when the user presses the enter key.
#[derive(Message)]
pub struct TextInputSubmitEvent {
    /// The text input that triggered the event.
    pub entity: Entity,
    /// The string contained in the text input at the time of the event.
    pub value: String,
}

/// Reads keyboard input and applies it to the focused text input's [`EditableText`].
///
/// Edits are queued onto [`EditableText::pending_edits`] and applied by Bevy's
/// own `apply_text_edits` system; we only read the resulting value back into
/// [`TextInputValue`] in [`update_value`].
fn keyboard(
    key_input: Res<ButtonInput<KeyCode>>,
    input_events: Res<Messages<KeyboardInput>>,
    mut input_reader: Local<MessageCursor<KeyboardInput>>,
    mut text_input_query: Query<(
        Entity,
        &TextInputSettings,
        &TextInputInactive,
        &mut TextInputValue,
        &mut TextInputCursorTimer,
        &mut EditableText,
    )>,
    mut submit_writer: MessageWriter<TextInputSubmitEvent>,
    navigation: Res<TextInputNavigationBindings>,
) {
    if input_reader.clone().read(&input_events).next().is_none() {
        return;
    }

    // collect actions that have all required modifiers held
    let valid_actions = navigation
        .0
        .iter()
        .filter(|(_, TextInputBinding { modifiers, .. })| {
            modifiers.iter().all(|m| key_input.pressed(*m))
        })
        .map(|(action, TextInputBinding { key, .. })| (*key, action));

    let select = key_input.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    for (input_entity, settings, inactive, mut text_input, mut cursor_timer, mut editor) in
        &mut text_input_query
    {
        if inactive.0 {
            continue;
        }

        let mut submitted_value = None;

        for input in input_reader.clone().read(&input_events) {
            if !input.state.is_pressed() {
                continue;
            };

            if let Some((_, action)) = valid_actions
                .clone()
                .find(|(key, _)| *key == input.key_code)
            {
                use TextInputAction::*;
                let mut timer_should_reset = true;

                let edit = match action {
                    CharLeft => Some(TextEdit::Left(select)),
                    CharRight => Some(TextEdit::Right(select)),
                    TextStart => Some(TextEdit::TextStart(select)),
                    TextEnd => Some(TextEdit::TextEnd(select)),
                    LineStart => Some(TextEdit::LineStart(select)),
                    LineEnd => Some(TextEdit::LineEnd(select)),
                    WordLeft => Some(TextEdit::WordLeft(select)),
                    WordRight => Some(TextEdit::WordRight(select)),
                    LineUp => Some(TextEdit::Up(select)),
                    LineDown => Some(TextEdit::Down(select)),
                    DeletePrev => Some(TextEdit::Backspace),
                    DeleteNext => Some(TextEdit::Delete),
                    NewLine if settings.multiline => Some(TextEdit::Insert("\n".into())),
                    // NewLine here only fires in single-line mode (guarded arm above takes priority)
                    Submit | NewLine => {
                        let retain = settings.retain_on_submit;
                        if retain {
                            submitted_value = Some(text_input.0.clone());
                        } else {
                            submitted_value = Some(std::mem::take(&mut text_input.0));
                        };
                        timer_should_reset = false;
                        if retain {
                            None
                        } else {
                            editor.clear();
                            None
                        }
                    }
                    SelectAll => Some(TextEdit::SelectAll),
                    Cut => Some(TextEdit::Cut),
                    Copy => Some(TextEdit::Copy),
                    Paste => Some(TextEdit::Paste),
                    // Undo/redo are not provided by `EditableText` yet (see module docs).
                    Undo | Redo => None,
                };

                if let Some(edit) = edit {
                    editor.queue_edit(edit);
                }

                cursor_timer.should_reset |= timer_should_reset;
                continue;
            }

            match input.logical_key {
                Key::Space => {
                    editor.queue_edit(TextEdit::Insert(" ".into()));
                    cursor_timer.should_reset = true;
                }
                Key::Character(ref s) => {
                    editor.queue_edit(TextEdit::Insert(s.as_str().into()));
                    cursor_timer.should_reset = true;
                }
                _ => (),
            }
        }

        if let Some(value) = submitted_value {
            submit_writer.write(TextInputSubmitEvent {
                entity: input_entity,
                value,
            });
        }
    }

    input_reader.clear(&input_events);
}

/// TextInputPointerAction
#[derive(Debug, PartialEq)]
pub enum TextInputPointerAction {
    /// TextInputPointerAction
    Press,
    /// TextInputPointerAction
    Drag,
    /// TextInputPointerAction
    Release,
}

/// TextInputPointerEvent
#[derive(Message, Debug)]
pub struct TextInputPointerEvent {
    /// TextInputPointerEvent
    pub position: Vec2,
    /// TextInputPointerEvent
    pub action: TextInputPointerAction,
}

#[allow(clippy::too_many_arguments)]
fn pointer(
    mut events: MessageReader<TextInputPointerEvent>,
    mut last_action: Local<Option<(Entity, f32, usize)>>,
    mut buffers: Query<(
        &TextInputInactive,
        Entity,
        &mut EditableText,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    ui_scale: Res<UiScale>,
    time: Res<Time>,
) {
    for event in events.read() {
        let time = time.elapsed_secs();

        let Some((_, entity, mut editor, node, transform)) =
            buffers.iter_mut().find(|(inactive, ..)| !inactive.0)
        else {
            continue;
        };

        let click_count = last_action
            .filter(|(e, t, _)| {
                *e == entity && (*t > time - 0.25 || event.action == TextInputPointerAction::Drag)
            })
            .map(|(_, _, c)| c)
            .unwrap_or(0);

        // Map the screen-space pointer position into the editor's local text
        // layout space (origin at the top-left of the content box).
        let Some(local_pos) = transform.try_inverse().map(|inverse| {
            inverse.transform_point2(event.position / ui_scale.0) - node.content_box().min
        }) else {
            continue;
        };

        match event.action {
            TextInputPointerAction::Release => (),
            TextInputPointerAction::Press => {
                let edit = match click_count {
                    0 => TextEdit::MoveToPoint(local_pos),
                    1 => TextEdit::SelectWordAtPoint(local_pos),
                    _ => TextEdit::SelectLineAtPoint(local_pos),
                };
                editor.queue_edit(edit);
                *last_action = Some((entity, time, click_count + 1));
            }
            TextInputPointerAction::Drag => {
                if click_count > 0 {
                    editor.queue_edit(TextEdit::ExtendSelectionToPoint(local_pos));
                }
            }
        }
    }
}

/// Reads the current text out of each [`EditableText`] back into [`TextInputValue`].
fn update_value(
    mut input_query: Query<(&mut TextInputValue, &EditableText), Changed<EditableText>>,
) {
    for (mut text_input, editor) in &mut input_query {
        let value = editor.value().to_string();
        if text_input.0 != value {
            text_input.0 = value;
        }
    }
}

/// Pushes the latest [`TextInputValue`] into the [`EditableText`] when it is
/// changed externally (e.g. the app resets the field).
fn sync_settings(
    mut input_query: Query<
        (&TextInputValue, &TextInputSettings, &mut EditableText),
        Or<(Changed<TextInputValue>, Changed<TextInputSettings>)>,
    >,
) {
    for (value, settings, mut editor) in &mut input_query {
        editor.allow_newlines = settings.multiline;

        // Only overwrite the editor's text if the external value diverged. This
        // happens when the app sets `TextInputValue` directly; ordinary typing
        // flows the other way (editor -> value) via `update_value`.
        if editor.value().to_string() != value.0 {
            editor.editor.set_text(&value.0);
            editor.queue_edit(TextEdit::TextEnd(false));
        }
    }
}

fn create(
    trigger: On<Add, TextInputValue>,
    mut commands: Commands,
    query: Query<(
        &TextInputTextFont,
        &TextInputTextColor,
        &TextInputValue,
        &TextInputInactive,
        &TextInputSettings,
        &TextInputPlaceholder,
        &TextInputSelectionStyle,
    )>,
) {
    let target = trigger.event().entity;
    if let Ok((font, color, text_input, inactive, settings, placeholder, selection_style)) =
        &query.get(target)
    {
        // The placeholder is rendered as a separate absolutely-positioned child
        // so it can overlay the (empty) input.
        let placeholder_font = placeholder
            .text_font
            .clone()
            .unwrap_or_else(|| font.0.clone());

        let placeholder_color = placeholder
            .text_color
            .unwrap_or_else(|| placeholder_color(&color.0));

        let placeholder_visible = inactive.0 && text_input.0.is_empty();

        let placeholder_text = commands
            .spawn((
                Text::new(&placeholder.value),
                TextLayout::no_wrap(),
                placeholder_font,
                placeholder_color,
                Name::new("TextInputPlaceholderInner"),
                TextInputPlaceholderInner,
                if placeholder_visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ))
            .id();

        commands.entity(target).add_children(&[placeholder_text]);

        // Turn the input entity itself into a native editable-text node. Bevy's
        // `bevy_ui`/`bevy_ui_render` will render the text, cursor and selection
        // for us, styled by the `TextFont`/`TextColor`/`TextCursorStyle`
        // components inserted below.
        let mut editor = EditableText::new(&text_input.0);
        editor.allow_newlines = settings.multiline;

        commands.entity(target).insert((
            editor,
            font.0.clone(),
            color.0,
            TextLayout::linebreak(if settings.multiline {
                LineBreak::WordBoundary
            } else {
                LineBreak::NoWrap
            }),
            cursor_style(color, selection_style),
            TextInputInner,
            Name::new("TextInputInner"),
        ));
    }
}

fn cursor_style(
    color: &TextInputTextColor,
    selection_style: &TextInputSelectionStyle,
) -> TextCursorStyle {
    let base = TextCursorStyle::default();
    let selection_color = selection_style.background.unwrap_or(base.selection_color);
    TextCursorStyle {
        color: *color.0,
        selection_color,
        unfocused_selection_color: selection_color,
        selected_text_color: selection_style.color,
    }
}

fn sync_inactive(
    mut input_query: Query<
        (&TextInputInactive, &mut EditableText, &mut TextCursorStyle),
        Changed<TextInputInactive>,
    >,
) {
    for (inactive, mut editor, mut cursor) in &mut input_query {
        if inactive.0 {
            // Collapse selection and hide highlight while inactive.
            editor.queue_edit(TextEdit::CollapseSelection);
            cursor.unfocused_selection_color = Color::NONE;
        } else {
            cursor.unfocused_selection_color = cursor.selection_color;
        }
    }
}

fn show_hide_placeholder(
    input_query: Query<
        (&Children, &TextInputValue, &TextInputInactive),
        Or<(Changed<TextInputValue>, Changed<TextInputInactive>)>,
    >,
    mut vis_query: Query<&mut Visibility, With<TextInputPlaceholderInner>>,
) {
    for (children, text, inactive) in &input_query {
        let mut iter = vis_query.iter_many_mut(children);
        while let Some(mut inner_vis) = iter.fetch_next() {
            inner_vis.set_if_neq(if text.0.is_empty() && inactive.0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn sync_style(
    mut input_query: Query<
        (
            &TextInputTextFont,
            &TextInputTextColor,
            &TextInputSelectionStyle,
            &mut TextFont,
            &mut TextColor,
            &mut TextCursorStyle,
        ),
        Or<(
            Changed<TextInputTextFont>,
            Changed<TextInputTextColor>,
            Changed<TextInputSelectionStyle>,
        )>,
    >,
) {
    for (font, color, selection_style, mut text_font, mut text_color, mut cursor) in
        &mut input_query
    {
        text_font.clone_from(&font.0);
        *text_color = color.0;
        *cursor = cursor_style(color, selection_style);
    }
}

fn placeholder_color(color: &TextColor) -> TextColor {
    TextColor(color.with_alpha(color.alpha() * 0.25))
}
