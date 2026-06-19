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

Clean (no warnings) under dcl-shell with `cargo build`, `cargo build --examples`,
and `cargo build --no-default-features --features std`.
