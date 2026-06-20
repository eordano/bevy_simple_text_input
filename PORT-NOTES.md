# Port notes: bevy 0.16 -> 0.17

This branch (`0.17`) ports the robtfm fork of `bevy_simple_text_input` from
Bevy 0.16 to Bevy 0.17 (0.17.3 from crates.io).

## Cargo.toml

- Bumped `bevy` (both `[dependencies]` and `[dev-dependencies]`) from `0.16.0`
  to `0.17.3`.
- Added the `bevy_sprite` feature to the `bevy` dependency. In 0.17 some UI/text
  glue moved behind `bevy_sprite`; enabling it keeps the text rendering path
  available. (No `Text2d`/`Text2dShadow` is used directly by this crate, but the
  feature is the safe default given the migration guide's relocation note.)
- Added a direct `cosmic-text = "0.14"` dependency (see below).

## cosmic_text re-export removal

Bevy 0.17 removed the `bevy::text::cosmic_text` re-export. The crate uses several
cosmic-text types directly (`Action`, `Change`, `Cursor`, `Edit`, `Editor`,
`Selection`, `Motion`).

Fix: depend on `cosmic-text` directly, pinned to `0.14` to match the version
`bevy_text` 0.17.3 itself uses (so `CosmicBuffer.0`, `CosmicFontSystem.0`, and
`ComputedTextBlock::buffer()` interoperate with the same `Buffer`/`FontSystem`
types). Imports changed:

- `bevy::text::{... , cosmic_text::{...}}` -> `bevy::text::{...}` plus
  `use cosmic_text::{Action, Change, Cursor, Edit, Editor, Selection};`
- inline `bevy::text::cosmic_text::Motion` -> `cosmic_text::Motion`

`CosmicBuffer`, `CosmicFontSystem`, `ComputedTextBlock`, `LineBreak` are still
re-exported from `bevy::text` and were left as-is.

## Event vs Message split

0.17 splits buffered "events" (now Messages) from observer Events.
`TextInputSubmitEvent` and `TextInputPointerEvent` are buffered, so:

- `#[derive(Event)]` -> `#[derive(Message)]` on both types.
- `App::add_event::<T>()` -> `App::add_message::<T>()`.
- `EventWriter<T>` -> `MessageWriter<T>`, `EventReader<T>` -> `MessageReader<T>`.
- `Res<Events<KeyboardInput>>` -> `Res<Messages<KeyboardInput>>`.
- `Local<EventCursor<KeyboardInput>>` -> `Local<MessageCursor<KeyboardInput>>`
  (import moved from `bevy::ecs::event::EventCursor` to
  `bevy::ecs::message::MessageCursor`).

The public type *names* (`TextInputSubmitEvent`, `TextInputPointerEvent`,
`TextInputPointerAction`, ...) are unchanged, so downstream consumers only need
to switch their own `EventReader`/`EventWriter` to `MessageReader`/`MessageWriter`
as part of their own 0.17 migration.

## Observer API

- `Trigger<OnAdd, TextInputValue>` -> `On<Add, TextInputValue>` in the `create`
  observer.
- `trigger.target()` is deprecated for entity events. Replaced with reading the
  entity off the event: `trigger.event().entity` (the lifecycle `Add` event
  exposes a public `entity` field).

## Examples

- `BorderColor` became a per-side struct in 0.17 (fields `top`/`bottom`/`left`/
  `right`, no tuple `.0`). Updated examples:
  - construction `BorderColor(c)` -> `BorderColor::all(c)`
  - mutation `border_color.0 = c;` -> `border_color.set_all(c);`
  - `*border_color = c.into()` left as-is (the `From<impl Into<Color>>` impl
    still exists).
- `examples/focus.rs`: `Trigger<Pointer<Click>>` -> `On<Pointer<Click>>`, and
  `trigger.target()` -> `trigger.event().entity`.
- `examples/basic.rs`, `examples/multiline.rs`: `EventReader`/`EventWriter` ->
  `MessageReader`/`MessageWriter`.

## Incidental cleanup

`tasks::IoTaskPool` was imported unconditionally but only used under
`#[cfg(feature = "clipboard")]`, producing an `unused_imports` warning in a
`--no-default-features` build (pre-existing on 0.16). Dropped the import and
fully-qualified the two call sites as `bevy::tasks::IoTaskPool::get()` so every
feature configuration builds warning-free.

## Build status

Clean (no warnings) with `cargo build`, `cargo build --examples`,
and `cargo build --no-default-features --features std`.

---

# Port notes: bevy 0.17 -> 0.18

This branch (`0.18`) continues the port to Bevy 0.18 (0.18.1 from crates.io).

## Cargo.toml

- Bumped `bevy` (both `[dependencies]` and `[dev-dependencies]`) from `0.17.3`
  to `0.18.1`.
- Bumped the direct `cosmic-text` dependency from `0.14` to `0.16`, matching the
  version `bevy_text` 0.18.1 uses (`cosmic-text = { version = "0.16", features =
  ["shape-run-cache"] }`). This keeps `CosmicBuffer.0`, `CosmicFontSystem.0`, and
  `ComputedTextBlock::buffer()` interoperable with the same `Buffer`/`FontSystem`
  types, and the cosmic-text API surface this crate uses (`Action`, `Change`,
  `Cursor`, `Edit`, `Editor`, `Selection`, `Motion`) is unchanged across
  0.14 -> 0.16.
- `bevy_sprite` feature kept (still where some UI/text glue lives).

## TextUiReader item tuple gained LineHeight

In 0.18 `LineHeight` was removed from `TextFont` and is now its own component
required by `Text`/`Text2d`/`TextSpan`. As a knock-on, `TextUiReader::iter`
yields a 6-tuple that now includes the resolved `LineHeight`:

- `(Entity, usize, &str, &TextFont, Color)` ->
  `(Entity, usize, &str, &TextFont, Color, LineHeight)`

Fix (in `TextPositionFinder::cursor_entity`):

- `for (entity, _, text, _, _) in self.reader.iter(entity)` ->
  `for (entity, _, text, _, _, _) in self.reader.iter(entity)`

This crate never set `TextFont.line_height`, so no `TextFont` construction sites
needed changing, and it doesn't insert/read a `LineHeight` component directly.

## Items from the 0.17 -> 0.18 migration guide that did NOT apply

- **Entity-event immutability / `SetEntityEventTarget`**: the crate never builds
  entity events manually or calls `set_target`, so nothing changed. The `create`
  observer's `On<Add, TextInputValue>` + `trigger.event().entity` form (already
  used on the 0.17 branch) is still valid in 0.18.
- **`BorderRadius` -> `Node` field**: the crate and examples don't use
  `BorderRadius`.
- **`BorderColor`**: unchanged from 0.17 (still the per-side struct with
  `::all`/`::from`/`set_all`); examples already use the 0.17 form.
- **`TextLayoutInfo.section_rects` -> `run_geometry`**: not used; selection
  geometry is computed directly from cosmic-text `layout_runs()`/glyphs, not from
  `TextLayoutInfo`.
- **Observer `Trigger` -> `On`**: already migrated on the 0.17 branch (lib +
  `examples/focus.rs`); no further change.

## Build status (0.18)

Clean (no warnings) with `cargo build`, `cargo build --examples`,
and `cargo build --no-default-features --features std`. The only remaining
`cargo clippy` notes (a `.clone()` on a `Copy` `Option<TextColor>` and two
collapsible `if`s) are pre-existing on the 0.17 branch in unchanged code and are
unrelated to the port.

---

# Port notes: bevy 0.18 -> 0.19 (the Parley swap)

This branch (`0.19`) ports the crate to Bevy 0.19.0 (crates.io). This is **not**
a mechanical bump: Bevy 0.19 replaced its text engine, **Cosmic Text -> Parley**,
which removed every primitive the previous internals were built on.

## What broke

The 0.17/0.18 internals drove a `cosmic_text::Editor<'static>` (wrapped in a
private `CosmicEditor` component) for all cursor/selection/edit logic, and read
cursor geometry + selection rectangles by cloning Bevy's shaped buffer
(`ComputedTextBlock::buffer().0`, a `cosmic_text::Buffer`) into that editor and
walking `layout_runs()`/glyphs. In 0.19:

- `bevy::text::{CosmicBuffer, CosmicFontSystem}` — **removed**. There is no
  cosmic-text `Buffer`/`FontSystem` in `bevy_text` anymore.
- `ComputedTextBlock::buffer()` now returns `&parley::Layout<TextBrush>`, not a
  cosmic `Buffer`. `layout_runs()`, `.hit()`, `Cursor`, `cursor_position()`,
  `selection_bounds()`, `shape_as_needed()`, `Action`, `Change`, `Selection`,
  `Motion`, `Edit` — none of these exist on the Parley types.
- There is no longer any cosmic-text re-export from `bevy::text`.

In short: the entire editing core was non-portable as-is.

## Approach taken: rebuild on Bevy's native `EditableText`

Bevy 0.19 ships a first-class editable-text widget in `bevy_text`:
`EditableText` (a component wrapping a `parley::PlainEditor<TextBrush>`),
`TextEdit` (a deferred edit/navigation action enum), `TextCursorStyle`, and an
`apply_text_edits` system (added by `TextPlugin`, part of `DefaultPlugins`) that
applies queued edits using the same `FontCx`/`LayoutCx` Bevy uses for layout.
`bevy_ui` + `bevy_ui_render` render the text, **cursor and selection** natively
when an `EditableText` sits on a UI node.

Rather than (a) vendoring `cosmic-text` as a second, independent shaper — which
would diverge from Bevy's Parley fonts and produce wrong cursor/selection
geometry — or (b) hand-rolling cursor math against Parley, this branch
**re-implements the crate's internals as a thin facade over `EditableText`**:

- The `TextInput` entity *is* the `EditableText` node now. `create` inserts
  `EditableText::new(value)` plus `TextFont`/`TextColor`/`TextLayout`/
  `TextCursorStyle` on the target entity; Bevy renders text, cursor and
  selection. The old hand-built inner `Text` + 3 `TextSpan`s + manual
  selection-rectangle nodes + manual cursor node + blink/scroll systems are all
  gone (Bevy does this now).
- `keyboard` maps each bound `TextInputAction` to a `TextEdit` and calls
  `EditableText::queue_edit(...)`; `bevy_text::apply_text_edits` applies them.
- `pointer` maps `TextInputPointerEvent` press/drag/multi-click to
  `TextEdit::MoveToPoint` / `SelectWordAtPoint` / `SelectLineAtPoint` /
  `ExtendSelectionToPoint`, converting screen-space to the editor's local text
  space via `UiGlobalTransform` + `ComputedNode::content_box()` + `UiScale`.
- `update_value` reads `EditableText::value()` back into `TextInputValue`;
  `sync_settings` pushes externally-set `TextInputValue` into the editor and
  maps `multiline` onto `EditableText::allow_newlines`.
- Submit (Enter without Shift, in single-line mode) still fires
  `TextInputSubmitEvent` and clears/keeps text per `retain_on_submit`.
- Placeholder is still a separate absolutely-positioned child `Text` toggled by
  `show_hide_placeholder` (Parley/`EditableText` has no native placeholder).

### Required-components gotcha

`EditableText`'s render path needs `TextScroll`, `TextNodeFlags` and
`ContentSize` on the same entity, but Bevy only registers those as *required
components* inside `bevy_ui_widgets::EditableTextInputPlugin`. We deliberately do
**not** add that plugin (it installs Bevy's own keyboard/pointer/focus handling
via `bevy_picking`/`bevy_input_focus`, which would fight this crate's custom
input model and the app's `TextInputInactive`-based focus). Instead
`TextInputPlugin::build` registers those required components itself, so rendering
works under a plain `DefaultPlugins` app.

## Public API: preserved

All public items the app (`ui_core/text_entry.rs`) imports are unchanged in name
and shape: `TextInput`, `TextInputPlugin`, `TextInputSystem`, `TextInputValue`,
`TextInputSettings`, `TextInputInactive`, `TextInputCursorTimer`,
`TextInputTextFont`, `TextInputTextColor`, `TextInputSelectionStyle`,
`TextInputPlaceholder`, `TextInputSubmitEvent`, `TextInputPointerEvent`,
`TextInputPointerAction`, `TextInputNavigationBindings`, `TextInputAction`,
`TextInputBinding`. No downstream import changes are required for these types.

### One downstream change the *app* must make (not this crate)

`TextFont.font_size` changed from `f32` to the new `FontSize` enum in 0.19. The
app's `update_fontsize` system in `ui_core/src/text_entry.rs` does
`text.0.font_size = win_size * size.0;` (assigning an `f32`); on 0.19 that must
become `text.0.font_size = FontSize::Px(win_size * size.0);`. This is part of the
app's own 0.18->0.19 migration, not this crate. The crate's examples were updated
the same way (`font_size: 34.` -> `font_size: FontSize::Px(34.)`).

## Behaviour changes / gaps vs the 0.18 branch

These are limitations of the current native `EditableText`, surfaced honestly
rather than faked:

- **Masking (`TextInputSettings::mask_character`)**: NOT honoured. `EditableText`
  renders its own buffer and has no masking hook, so the `password` example now
  shows plaintext. The field is still accepted (no compile break) and is the
  obvious follow-up if password fields are needed (would require either a custom
  glyph brush or rendering a masked mirror string).
- **Undo / redo (`TextInputAction::Undo`/`Redo`)**: now no-ops. The old
  cosmic-text `Change`/`apply_change` undo stack has no `PlainEditor` equivalent
  yet; the bindings remain but do nothing.
- **`TextInputCursorTimer`**: retained as a component (the app inserts it) but no
  longer drives a custom blink system — Bevy's native cursor handles blinking
  via `EditableText::cursor_blink_period`.
- **`TextPositionFinder`** (public `SystemParam` that did cosmic hit-testing):
  removed. It depended entirely on `ComputedTextBlock::buffer().hit()` /
  `layout_runs()`, which no longer exist. It was not used by the app.
- The `clipboard` feature no longer pulls `copypwasmta`/`futures-lite`; copy/cut/
  paste are now native (`EditableText` + `bevy/system_clipboard`). The feature is
  kept (default-on) and just enables `bevy/system_clipboard`.

## Cargo.toml

- `bevy` bumped `0.18.1` -> `0.19.0` (deps + dev-deps). Added the
  `bevy_ui_render` feature (needed for native cursor/selection rendering).
- Dropped the direct `cosmic-text` dependency (no longer compatible / unused).
- Dropped `once_cell` (the lazy-editor trick it supported is gone).
- Dropped `copypwasmta` + `futures-lite`; `clipboard` feature now just enables
  `bevy/system_clipboard`.

## Build status (0.19)

Clean (no warnings) with `cargo build`, `cargo build --examples`,
and `cargo build --no-default-features --features std`. `cargo clippy
--all-targets` is also clean. Runtime/visual verification of the native
cursor/selection rendering against the full app (via `dcl-bevy`) is a follow-up;
the headless build environment here has no display to open a window.
