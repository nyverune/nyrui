# RFC-0001: Declarative Architecture and Runtime for Nyrui

- **Project:** Nyrui (`nyrui`)
- **RFC:** 0001
- **Status:** Accepted
- **Type:** Architecture / API Design
- **Language:** Rust
- **Terminal Backend:** Ratatui + Crossterm
- **Audience:** Framework maintainers and contributors
- **Last Updated:** 2026-11-12

---

## 1. Summary

Nyrui is a declarative, component-based Terminal User Interface (TUI) framework for Rust built on top of `ratatui` and `crossterm`. It targets medium-to-large terminal applications that outgrow manual immediate-mode rendering loops and ad-hoc state structs.

While Ratatui provides immediate-mode layout constraints, cell buffers, and primitive widgets, building complex applications on top of it requires reinventing component state persistence, focus traversal, event propagation, modal stacking, form validation, and async task coordination. Nyrui provides a retained component runtime and a hybrid reactive/manual update model while delegating terminal buffer diffing and drawing to Ratatui.

The architecture is structured into five layers:

1. **Declarative Surface (`ui!` & Builders):** A procedural macro (`ui!`) and an equivalent typed builder API that both lower into the same intermediate node representation.
2. **UI Description Layer (`UiNode`):** Lightweight, short-lived value trees describing desired component hierarchies, properties, keys, and layout constraints.
3. **Component Runtime:** A single-threaded generational arena maintaining persistent component instances, local state, lifecycle hooks, and structural reconciliation.
4. **UI Engine:** The event loop, fine-grained reactive graph (`Signal`, `Computed`), dirty-node scheduler, focus manager, layout cache (for hit-testing and scrolling), router, and overlay stack.
5. **Ratatui Renderer:** A rendering pass that resolves the reconciled tree against `ratatui::Frame` and `ratatui::layout::Rect`, providing zero-cost escape hatches to native `ratatui::widgets::Widget` implementations.

---

## 2. Motivation and Design Rationale

### 2.1 The Immediate-Mode Gap in Large TUIs

Ratatui intentionally restricts its scope to immediate-mode rendering: given a `Frame` and a `Rect`, the application draws widgets for the current tick. In small utilities, keeping all state in a single `App` struct and matching on `KeyEvent` in a giant `match` block works well. As an application scales to multiple views, nested lists, text inputs, asynchronous background jobs, and stacked confirmation dialogs, three structural problems emerge:

1. **Disjointed State and View Hierarchy:** Because immediate-mode widgets are ephemeral, stateful details (scroll offsets, cursor positions, expanded tree nodes, in-flight validation errors) must be hoisted into parent structs. Adding or reusing a composite widget requires wiring both its rendering call and its state fields manually through every ancestor.
2. **Unstructured Event Routing and Focus:** Without a retained tree and layout geometry cache, routing keyboard shortcuts or mouse clicks requires manual coordinate checks and brittle boolean flags (`app.is_modal_open`, `app.active_tab == 2`).
3. **All-or-Nothing Rendering Coupling:** Business logic, state mutation, and terminal drawing become interleaved unless the developer designs a custom Elm/TEA or signal architecture from scratch.

### 2.2 Why Not a Traditional Virtual DOM or Custom Cell Buffer?

Web frameworks use a Virtual DOM because mutating browser DOM nodes is expensive relative to allocating JavaScript objects. In a terminal application backed by Ratatui, the "DOM" is an in-memory 2D array of `Cell` structs (`ratatui::buffer::Buffer`) that Ratatui already diffs cell-by-cell at the end of every frame before flushing ANSI escape sequences to `crossterm`.

Adding a second cell-level diffing engine in Nyrui would duplicate Ratatui's core mechanism and waste memory. Conversely, recreating the entire component tree from scratch on every keystroke loses local component state. Nyrui therefore separates **component reconciliation** from **terminal rendering**:

- **Component Reconciliation (Retained):** The runtime maintains a persistent arena of mounted components. When a component's state or subscribed `Signal` becomes dirty, only that component (and affected children) re-evaluates its `render()` method to produce a new `UiNode` subtree, preserving sibling and ancestor state.
- **Frame Drawing (Immediate):** When the scheduler flushes a dirty frame, the runtime walks the active tree, computes and caches `Rect` geometry using Ratatui's layout solver, and draws directly into `ratatui::Frame`.

### 2.3 Explicit Non-Goals

- **Multi-threaded UI Tree Mutation:** Component instances and UI descriptions are strictly confined to the UI thread (`!Send`, `!Sync`). Background threads and Tokio tasks communicate with the UI thread via channels or thread-safe signal handles.
- **CSS Parser or Custom Layout Solver:** Layout maps directly to Ratatui constraints (`Length`, `Percentage`, `Min`, `Max`, `Fill`, `Ratio`). Styling uses typed semantic design tokens, not runtime CSS selectors.
- **Hiding Ratatui:** Nyrui is a layer over Ratatui, not a walled garden. Any `ratatui::widgets::Widget` or custom `Frame` drawing closure can be embedded directly as a leaf node.

---

## 3. Core Architectural Specifications

### 3.1 Single-Threaded UI Confinement (`!Send` Arena)

All component instances, lifecycle hooks, local states, and UI callbacks live exclusively on the UI thread. This is a foundational constraint with major ergonomic and performance payoffs in Rust:

- **No `Arc<Mutex<...>>` Tax:** Component state is mutated via exclusive `&mut Self::State` references managed by the runtime arena during event and update passes.
- **Ergonomic Callbacks:** Event callbacks (`Callback<E>`) and slot render functions can wrap `Rc<dyn Fn(...)>` rather than requiring `Arc<dyn Fn(...) + Send + Sync>`, allowing closures to share local UI handles without thread-safety boilerplate.
- **Zero-Lock Reactive Graph:** Signals and computed values read and write through a thread-local reactive runtime without atomic contention on the render path.

Cross-thread communication is restricted to explicit boundaries: asynchronous tasks spawned on the Tokio runtime send typed messages or update thread-safe channels that the UI event loop drains at the start of each tick.

### 3.2 Dual Construction API: `ui!` Macro and Typed Builders

Nyrui provides two interchangeable ways to construct `UiNode` trees: the `ui!` procedural macro and fluent Rust builders. `ui!` is purely syntactic sugar that expands at compile time into builder invocations.

```rust
// Declarative macro syntax
ui! {
    Column(spacing: 1, padding: Padding::horizontal(2)) {
        Text("Nyrui Dashboard", style: Theme::heading())

        if self.is_authenticated {
            Button(
                label: "Deploy",
                disabled: !self.can_deploy(),
                on_press: ctx.callback(Msg::Deploy),
            )
        }

        for project in &self.projects {
            ProjectRow(key: project.id, project: project.clone())
        }
    }
}
```

The macro above desugars directly to the builder API:

```rust
Column::new()
    .spacing(1)
    .padding(Padding::horizontal(2))
    .child(Text::new("Nyrui Dashboard").style(Theme::heading()))
    .child_opt(self.is_authenticated.then(|| {
        Button::new("Deploy")
            .disabled(!self.can_deploy())
            .on_press(ctx.callback(Msg::Deploy))
    }))
    .children(
        self.projects
            .iter()
            .map(|project| ProjectRow::new(project.clone()).with_key(project.id)),
    )
    .into_node()
```

Because `ui!` lowers directly to builder calls, type errors point to the exact property span, IDE rust-analyzer completions work on builder methods, and dynamic views can mix `ui!` blocks and builder chains freely.

### 3.3 Hybrid State Model: Fine-Grained Signals + Manual Invalidation

Forcing every boolean toggle or cursor index into a reactive wrapper adds unnecessary indirection, while relying purely on manual dirty flags scales poorly when state is shared across distant subtrees. Nyrui supports both models natively:

1. **Component-Local State + Manual Mutation:** A component mutates `self` (or its `State`) inside its `update` handler in response to an event or message. The runtime automatically marks a component dirty when its `update` method returns `ShouldRender::Yes` (or when `ctx.invalidate()` is called explicitly).
2. **Fine-Grained Reactive State (`Signal<T>` & `Computed<T>`):** Shared or granular state uses arena-allocated `Signal<T>` handles (`Copy` IDs into the reactive graph). Reading a signal inside a component's `render()` method or inside a `Computed<T>` closure automatically subscribes that component `NodeId` to the signal. Calling `signal.set(new_val)` marks only the subscribed `NodeId`s dirty in the render scheduler.

---

## 4. Architecture and Data Flow

### 4.1 Layer Breakdown

```text
┌─────────────────────────────────────────────────────────────┐
│                   Application / Views                       │
│               ui! { ... }   &   Component Builders          │
└──────────────────────────────┬──────────────────────────────┘
                               │ produces
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                UI Description Layer (UiNode)                │
│   Element<C> · Primitive · Fragment · Slot · Key · Layout   │
└──────────────────────────────┬──────────────────────────────┘
                               │ reconciled into
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                     Component Runtime                       │
│   SlotMap<NodeId, InstanceNode> · State · Lifecycle Hooks   │
└──────────────────────────────┬──────────────────────────────┘
                               │ driven by
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                         UI Engine                           │
│   Event Loop · Reactive Graph · Dirty Scheduler · Focus     │
│   Hit-Test Geometry Cache · Overlay Stack · Router          │
└──────────────────────────────┬──────────────────────────────┘
                               │ renders to
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                      Ratatui Renderer                       │
│         ratatui::Frame · Layout Solver · Buffer Diff        │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Frame Lifecycle and Update Loop

The runtime executes an event-driven loop (on top of `tokio::select!`) rather than a fixed-fps spin loop. A frame is scheduled only when input arrives, an async task delivers a message, a timer ticks, or a `Signal` is mutated.

```text
[Crossterm Input / Async Channel / Timer]
                   │
                   ▼
        1. Event Dispatch Phase
   (Capture -> Target -> Bubble / Keybindings)
                   │
                   ▼
        2. State & Signal Update
   (Component::update & Signal::set batching)
                   │
                   ▼
        3. Dirty Node Reconciliation
   (Pop dirty NodeIds ordered by tree depth;
    run Component::render -> diff child UiNodes)
                   │
                   ▼
        4. Layout & Geometry Pass
   (Compute Ratatui Rects for active tree & overlays;
    store Rect per NodeId for mouse hit-testing)
                   │
                   ▼
        5. Terminal Draw Pass
   (terminal.draw(|frame| renderer.draw(frame)))
```

Reconciling dirty nodes in **top-down depth order** (using a min-heap keyed by tree depth) guarantees that if both a parent and its child are marked dirty in the same batch, the parent reconciles first; if the parent updates or unmounts the child, the child is never redundantly rendered.

---

## 5. Declarative API & `ui!` Macro Specification

### 5.1 Grammar and Syntax Rules

The `ui!` procedural macro (`nyrui-macros`) parses a deterministic Rust-native grammar using `syn`:

- **Component Invocation:** `ComponentName(prop1: expr1, prop2: expr2) { ...children... }`. Parentheses hold props; braces hold children and named slots. Either parentheses or braces may be omitted if empty (`Divider()`, `Row { Text("a") }`).
- **Positional Primary Prop:** Primitive components with an obvious primary value (such as `Text("Hello", style: ...)` or `Button("Save", on_press: ...)`) accept the primary prop as the first unnamed argument before named `key: value` pairs.
- **Native Control Flow:** Rather than inventing wrapper components like `<Show>` or `<For>`, `ui!` directly supports standard Rust control-flow expressions inside child blocks:
  - `if cond { ... } else if cond2 { ... } else { ... }`
  - `match expr { pat => { ... }, ... }`
  - `for pat in iter_expr { ... }`
- **Keys:** Any component accepts `key: expr`, where `expr` implements `Into<Key>`. Inside `for` loops, providing `key:` binds the reconciliation identity of each generated child node.

### 5.2 Typed Props and Compile-Time Verification

Every component defines an associated `Props` type (or derives it via `#[derive(Props)]` / `#[component]`):

```rust
#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    pub label: SharedStr,
    #[prop(default = false)]
    pub disabled: bool,
    #[prop(default = ButtonVariant::Primary)]
    pub variant: ButtonVariant,
    #[prop(optional)]
    pub on_press: Option<Callback<()>>,
}
```

Compile-time guarantees enforced by the builder pattern expanded by `ui!`:

- **Required vs. Optional Props:** Using a typestate builder pattern (or struct literal expansion with defaults for optional fields), omitting a required prop (`label`) produces a standard Rust compiler error at the call site.
- **Zero Stringly-Typed Maps:** Props are concrete structs, never `HashMap<String, Value>`.
- **Automatic Conversion:** Prop setters accept `impl Into<T>` (e.g., `&'static str`, `String`, or `Arc<str>` into `SharedStr`), avoiding `.to_string()` clutter in view code.

### 5.3 Slots: Default, Named, and Scoped

Slots allow container components to accept structured child views:

1. **Default Slot (`children: Children`):** Any child nodes placed directly inside `Component { ... }` populate the `children` field on the component's `Props`.
2. **Named Slots (`slot name { ... }`):** Regions such as dialog headers or footers are passed via `slot <ident> { ... }`, which lowers to `.slot_<ident>(ui! { ... })` on the parent's builder:

   ```rust
   ui! {
       Dialog(title: "Delete Identity") {
           slot body {
               Text("Are you sure you want to delete this SSH key?")
           }
           slot footer {
               Button("Cancel", variant: ButtonVariant::Ghost, on_press: on_cancel)
               Button("Delete", variant: ButtonVariant::Danger, on_press: on_confirm)
           }
       }
   }
   ```

3. **Scoped Slots (`slot name |item| { ... }`):** When a component (such as `VirtualList<T>` or `DataTable<T>`) controls iteration or layout over internal items and delegates row rendering to the caller, it accepts a `SlotFn<T>`:

   ```rust
   pub struct SlotFn<T>(Rc<dyn Fn(&T) -> UiNode>);
   ```

   Because the runtime is single-threaded (`!Send`) and `UiNode` owns its data, `SlotFn<T>` does not borrow stack references across frames; captured values are either `Copy` handles (`Signal<U>`, `Callback<M>`) or cloned `Rc`/`Arc` data.

   ```rust
   ui! {
       DataTable(rows: &self.identities) {
           slot row |identity: &SshIdentity| {
               Row {
                   Text(identity.name.clone())
                   Text(identity.fingerprint.clone(), style: Theme::muted())
               }
           }
       }
   }
   ```

---

## 6. Component Model and Runtime Storage

### 6.1 Public `Component` Trait

Nyrui separates **stateless functional components** from **stateful managed components**, unified under the `Component` trait:

```rust
pub trait Component: 'static {
    type Props: Props;
    type Message: 'static;

    /// Initializes the component instance when mounted into the tree.
    fn create(props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self;

    /// Called when parent re-renders with new Props.
    /// Returns whether the component needs to re-render.
    fn update_props(
        &mut self,
        new_props: &Self::Props,
        ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        let _ = (new_props, ctx);
        ShouldRender::Yes
    }

    /// Processes a component-local message or action triggered by events/tasks.
    fn update(
        &mut self,
        msg: Self::Message,
        ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        let _ = (msg, ctx);
        ShouldRender::No
    }

    /// Produces the declarative UI description for the current state and props.
    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode;

    /// Lifecycle hooks with default no-op implementations.
    fn on_mount(&mut self, _ctx: &mut ComponentContext<Self>) {}
    fn on_unmount(&mut self, _ctx: &mut ComponentContext<Self>) {}
    fn on_focus(&mut self, _ctx: &mut ComponentContext<Self>) {}
    fn on_blur(&mut self, _ctx: &mut ComponentContext<Self>) {}
}
```

For simple stateless components that only transform `Props -> UiNode`, a `#[component]` attribute macro on a plain function generates the boilerplate `Component` implementation with `type Message = ()` and a zero-sized state struct:

```rust
#[component]
pub fn Badge(label: SharedStr, kind: BadgeKind) -> UiNode {
    ui! {
        Container(style: kind.badge_style()) {
            Text(label)
        }
    }
}
```

### 6.2 Type-Erased Generational Arena Storage

To store heterogeneous components in a single contiguous tree without exposing `Box<dyn Any>` to users, the runtime uses a generational arena (`slotmap::SlotMap<NodeId, InstanceNode>`) paired with an internal object-safe wrapper trait `ErasedComponent`:

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct NodeId {
    pub index: u32,
    pub generation: NonZeroU32,
}

pub(crate) struct InstanceNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub type_id: TypeId,
    pub key: NodeKey,
    pub depth: u16,
    pub rect: ratatui::layout::Rect, // Cached during layout pass
    pub flags: NodeFlags,            // DIRTY | MOUNTED | FOCUSED | FOCUSABLE
    pub subscriptions: SmallVec<[SignalId; 4]>,
    pub instance: Option<Box<dyn ErasedComponent>>, // Temporarily taken during update/render
}
```

Note the `Option<Box<dyn ErasedComponent>>` pattern: when the runtime calls `update()` or `render()` on `node_id`, it temporarily `.take()`s the `Box<dyn ErasedComponent>` out of the arena slot, passes `&mut Runtime` (wrapped in `ComponentContext`) into the method so the component can freely allocate signals, emit parent callbacks, or query context without aliasing `&mut arena`, and places the `Box` back into the slot upon return. This eliminates `RefCell` borrow panics across re-entrant context operations.

### 6.3 Identity and Reconciliation Algorithm

Each `UiNode` carries a `NodeKey`:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Key {
    U64(u64),
    I64(i64),
    StaticStr(&'static str),
    SharedStr(Arc<str>),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum NodeKey {
    /// Explicit developer-provided key (`key: project.id`).
    Explicit(Key),
    /// Implicit positional key scoped to (branch_id, child_index).
    Positional { scope: u16, index: u32 },
}
```

Notice the `scope: u16` inside `Positional`: when `if / else` branches expand inside `ui!`, each syntactic branch is assigned a distinct compile-time `scope` ID. Consequently, switching from an `if` branch containing a `TextInput` to an `else` branch also containing a `TextInput` will **not** accidentally reuse the same instance state unless the developer explicitly assigns them the same `Key`.

When reconciling a parent's old children (`&[NodeId]`) against a new list of `UiNode` descriptions:

1. **Fast Path (Common Prefix / Suffix):** Advance from the start and end while `(old.type_id, old.key) == (new.type_id, new.key)`. Matching nodes update their `Props` in-place via `ErasedComponent::update_props`. If `new_props == old_props` (checked via `PartialEq` where implemented), subtree re-rendering is skipped unless the child itself is flagged `DIRTY`.
2. **Remaining Middle Slice:** Build a lookup table of remaining old children keyed by `(TypeId, NodeKey)` (using `SmallVec` linear scan for $\le 8$ children, `FxHashMap` for $> 8$ children):
   - **Match found:** Reuse the existing `NodeId`, move it to the new child order, and update its props.
   - **No match found:** Allocate a new `InstanceNode` in the arena, invoke `Component::create`, and schedule `on_mount`.
   - **Duplicate explicit key among siblings:** Log a structured diagnostic warning in release builds and `debug_assert!` panic in debug/test builds.
3. **Unmount Leftovers:** Any old `NodeId` not matched in step 1 or 2 is recursively unmounted in post-order (children before parent): `on_unmount` runs, signal subscriptions are detached, focus is evacuated if held, and the `NodeId` is removed from the arena (incrementing its generation so any stale `NodeId` in queued async messages is safely ignored).

---

## 7. Fine-Grained Reactivity and Scheduling

### 7.1 Arena-Backed Reactive Graph (`Signal` & `Computed`)

Rather than reference-counted heap allocations per signal, reactive primitives live in a `ReactiveGraph` owned by the runtime and are referenced by lightweight `Copy` handles:

```rust
pub struct Signal<T: 'static> {
    id: SignalId,
    _marker: PhantomData<T>,
}

pub struct Computed<T: 'static> {
    id: ComputedId,
    _marker: PhantomData<T>,
}
```

- **Automatic Dependency Tracking:** When the runtime executes `Component::render(node_id)` or evaluates a `Computed` closure, it pushes the current observer (`ObserverId::Node(node_id)` or `ObserverId::Computed(computed_id)`) onto an internal tracking stack. Calling `signal.get(ctx)` (or `signal.with(ctx, |val| ...)`) records a bidirectional edge between `SignalId` and the active `ObserverId`.
- **Automatic Cleanup:** Before a component re-renders or when it unmounts, its previous incoming signal edges (`InstanceNode::subscriptions`) are cleared, preventing memory leaks or stale wakeups from conditional branches that are no longer rendered.
- **Lazy Push-Pull `Computed`:** Mutating a `Signal` (`signal.set(ctx, value)`) checks equality (`PartialEq` when available) and, if changed, pushes a `DIRTY` flag down to dependent `Computed` nodes and pushes subscribed `NodeId`s into the scheduler's dirty queue. `Computed` values are only recomputed lazily when read during the subsequent render pass.
- **Cycle Detection:** If a `Computed` evaluation attempts to read itself transitively, the tracking stack detects the duplicate `ObserverId::Computed` and panics with a clear dependency-cycle trace.

### 7.2 Batching and Render Coalescing

All mutations triggered during a single input event, action execution, or message batch are coalesced automatically. Calling `signal.set(...)` or `ctx.invalidate()` never triggers an immediate synchronous draw; it simply inserts the affected `NodeId` into `Scheduler::dirty_nodes`. Once the input event and all currently queued async messages have been processed, the scheduler runs a single reconciliation and render pass.

---

## 8. Event System, Callbacks, and Keybindings

### 8.1 Event Routing: Capture, Target, Bubble

Terminal input events from `crossterm::event::Event` are normalized into `UiEvent`:

```rust
pub enum UiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize { width: u16, height: u16 },
}
```

When an event arrives, the engine determines the target `NodeId`:

- **Keyboard & Paste Events:** Target is the currently focused `NodeId` (or the active modal root if no inner child has focus; or the application root if nothing is focused).
- **Mouse Events:** Target is the deepest visible `NodeId` whose cached layout `Rect` contains `(mouse.column, mouse.row)`, respecting overlay stacking order (an active modal with a backdrop blocks hit-testing on underlying views).

The event then traverses the ancestor path `[root, ..., parent, target]`:

1. **Capture Phase (`root -> parent`):** Ancestors with capture listeners (e.g., global or modal keybinding interceptors) inspect the event first. Any handler may return `EventPropagation::Stop` to consume the event early.
2. **Target Phase (`target`):** The target component handles the event.
3. **Bubble Phase (`parent -> root`):** If not consumed (`EventPropagation::Continue`), the event bubbles up to the root.

### 8.2 Callbacks Without Lifetime Contagion

Props passed into children must be `'static` so they can be stored in the arena across renders. A child component communicates upward through `Callback<In>`:

```rust
pub struct Callback<In: 'static>(Rc<dyn Fn(In)>);

impl<C: Component> ComponentContext<'_, C> {
    /// Creates a callback that sends a message to this component's `update` method.
    pub fn callback<In: 'static>(
        &self,
        f: impl Fn(In) -> C::Message + 'static,
    ) -> Callback<In> { ... }
}
```

Internally, `ctx.callback(...)` captures only the parent's `NodeId` (8 bytes, `Copy`) and a handle to the runtime's local message queue. When invoked by a child (`on_press.emit(())`), it pushes `(parent_node_id, f(input))` onto the runtime queue. If the parent has already been unmounted by the time the callback fires, the generational `NodeId` lookup cleanly no-ops.

### 8.3 Centralized Contextual Keybindings

Instead of hardcoding `if key.code == KeyCode::Char('q')` inside individual widgets, Nyrui provides a `Keymap` registry scoped by context layers (`Global`, `View`, `Overlay`, `Component`):

```rust
Keymap::new()
    .bind("ctrl+p", CommandId::OpenCommandPalette)
    .bind_in(KeyScope::View("identities"), "d", IdentityAction::DeleteSelected)
    .bind_in(KeyScope::View("identities"), "a", IdentityAction::CreateNew)
```

When a `TextInput` or `TextArea` is focused and actively editing text, printable single-character bindings (`"d"`, `"a"`) in ancestor `View`/`Global` scopes are automatically suppressed so the user can type normally, while modified chords (`"ctrl+p"`, `"esc"`) continue to match during the capture/bubble phases.

---

## 9. Focus Management and Overlays

### 9.1 Focus Manager

The runtime tracks focus using `NodeId` rather than references:

- **Focus Stack (`Vec<FocusContext>`):** Each layer (the base view and each stacked modal overlay) maintains its own `active_focus: Option<NodeId>` and ordered list of focusable `NodeId`s collected during the tree traversal pass.
- **Traversal (`Tab` / `Shift+Tab`):** Moves focus across nodes marked `NodeFlags::FOCUSABLE` (excluding `disabled` nodes) ordered first by explicit `tab_index: i16`, then by pre-order tree traversal.
- **Unmounted Focus Recovery:** If the currently focused `NodeId` is unmounted during reconciliation, focus falls back to the nearest focusable sibling, or its parent focus group, without losing keyboard interactivity.

### 9.2 Overlay Manager and Focus Trapping

Modals, dialogs, dropdown menus, command palettes, and toast notifications are rendered through the runtime's `OverlayManager` rather than requiring parent components to reserve z-index layout space:

```rust
let handle = ctx.overlays().open_modal(
    ModalConfig::new().trap_focus(true).close_on_escape(true),
    ui! { ConfirmDeleteModal(identity_id: id) },
);
```

- **Stacking Order:** Overlays are reconciled and rendered as separate root branches on top of the main view tree after the main view's draw pass completes, using `ratatui::widgets::Clear` over each overlay's computed `Rect` before drawing the overlay content.
- **Focus Trapping & Restoration:** Opening an overlay with `trap_focus(true)` pushes a new `FocusContext` onto the focus stack, confining `Tab` navigation and mouse hit-testing to the overlay's subtree. Closing the overlay pops the stack and automatically restores focus (`on_focus`) to the exact `NodeId` that was focused before the overlay opened (if still mounted).

---

## 10. Layout Engine and Ratatui Interoperability

### 10.1 Layout Primitives and Geometry Caching

Nyrui's layout primitives (`Row`, `Column`, `Grid`, `Stack`, `Container`, `SplitPane`, `ScrollArea`) compile directly to `ratatui::layout::{Layout, Constraint, Direction, Flex}` operations during the Layout Pass (Phase 4 of the frame cycle):

| Layout Component | Lowering Strategy                                                                                  |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| `Row` / `Column` | Evaluates `ratatui::layout::Layout::default().direction(...).constraints(...).split(area)`.        |
| `Grid`           | Nested row/column constraint splits with shared track definitions.                                 |
| `Stack`          | Assigns the same `Rect` to children in z-order, optionally clearing background regions.            |
| `ScrollArea`     | Maintains `scroll_offset: (u16, u16)` in component state and renders into a clipped viewport.      |
| `SplitPane`      | Stateful two-pane container with draggable/resizable ratio stored in local state.                  |

During the layout pass, the computed `Rect` is written into `InstanceNode::rect` for every mounted node. This single pass serves three purposes:
1. It feeds exact `Rect` coordinates to the subsequent Ratatui draw pass.
2. It enables $O(\text{depth})$ spatial mouse hit-testing without widget-specific coordinate math.
3. It notifies components implementing `on_resize(old_rect, new_rect)` when their allocated viewport changes.

### 10.2 Direct Ratatui Escape Hatches

Applications can drop down to raw Ratatui at any point in the component tree using two public primitives:

1. **`NativeWidget<W>`:** Wraps any type implementing `ratatui::widgets::Widget` (or `StatefulWidget` with state managed by a parent component):

   ```rust
   ui! {
       NativeWidget(
           constraint: Constraint::Min(10),
           widget: ratatui::widgets::Sparkline::default().data(&self.metrics),
       )
   }
   ```

2. **`CanvasDraw`:** Accepts a rendering closure `Fn(&mut ratatui::Frame<'_>, ratatui::layout::Rect)` invoked synchronously during Phase 5 (Terminal Draw Pass). Because the closure executes only during the draw pass, `ratatui::Frame<'_>` lifetimes never leak into `UiNode` or the persistent arena.

---

## 11. Styling, Context, Forms, and Navigation

### 11.1 Semantic Theme and Token System

Styling is built around a typed `Theme` struct exposed via context and resolved to `ratatui::style::{Style, Color, Modifier}`:

```rust
pub struct Theme {
    pub palette: SemanticPalette,
    pub borders: BorderSet,
    pub components: ComponentStyles,
}

pub struct SemanticPalette {
    pub bg_primary: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub fg_primary: Color,
    pub fg_muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub border: Color,
    pub border_focused: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub info: Color,
}
```

Themes can be loaded/serialized via TOML and switched at runtime (updating a root `Signal<Theme>` so subscribed components re-render automatically). Color resolution degrades gracefully from 24-bit TrueColor (`Color::Rgb`) to 256-color ANSI (`Color::Indexed`) or 16-color ANSI based on terminal capability detection at startup.

### 11.2 Scoped Context Injection (`Provider<T>`)

To avoid prop-drilling shared services, themes, or form states through intermediate layout components, any subtree can provide a typed value `T: Clone + 'static` (typically an `Arc<Service>`, `Signal<T>`, or handle):

```rust
// Ancestor provides context:
ctx.provide_context(self.ssh_client.clone());

// Any descendant retrieves it in O(depth) with caching:
let ssh_client = ctx.use_context::<Arc<SshClient>>()
    .expect("SshClient context must be provided by root view");
```

### 11.3 Typed Forms (`FormState<T>`)

Forms bind structured input state to field components with composable validation:

- Each `FieldState<V>` tracks `value: V`, `initial: V`, `touched: bool`, `dirty: bool`, and `errors: Vec<SharedStr>`.
- Validation rules (`Validator<V>`) run according to the configured trigger (`OnChange`, `OnBlur`, `OnSubmit`) and integrate directly with the shared `Validate` contract defined in [RFC-0002](file:////wsl.localhost/archlinux/root/nyverune/nyrui/docs/rfcs/0002-dual-interface-architecture.md) so TUI forms and CLI commands enforce identical domain invariants.

### 11.4 Typed Router

Navigation between top-level views uses an enum-based `Routable` trait rather than untyped URL strings:

```rust
#[derive(Clone, PartialEq, Eq)]
pub enum AppRoute {
    IdentityList,
    IdentityDetail { id: IdentityId },
    Settings,
}
```

`Router<R>` manages a history stack (`push`, `replace`, `back`, `forward`), navigation guards (e.g., prompting before leaving a dirty form), and focus restoration when returning to a previous route. By default, inactive routes are unmounted on navigation; routes annotated with `#[route(keep_alive)]` remain mounted in the arena with rendering and event dispatch suspended.

---

## 12. Workspace and Crate Architecture

To keep compile times fast and prevent circular dependencies while avoiding premature crate fragmentation, the repository uses a Cargo workspace with a consolidated initial crate layout:

```text
nyrui/
├── Cargo.toml
├── crates/
│   ├── nyrui-core/        # NodeId, Key, UiNode, Component trait, Signal/ReactiveGraph,
│   │                      # Arena runtime, Reconciler, Layout pass, Event dispatch, Theme
│   ├── nyrui-macros/      # Proc-macros: ui!, #[component], #[derive(Props)], #[action]
│   ├── nyrui-widgets/     # Standard primitive and composite widgets (Button, Input, Table, etc.)
│   ├── nyrui-cli/         # CLI adapter and action runner (see RFC-0002)
│   └── nyrui/             # Public facade crate re-exporting core, macros, widgets, and cli
└── examples/
```

**Dependency Invariants:**
1. `nyrui-core` never depends on `nyrui-widgets` or `nyrui-cli`.
2. `nyrui-macros` is a pure `proc-macro` crate emitting code targeting `nyrui_core` paths.
3. Internal subsystems inside `nyrui-core` (`reactivity`, `runtime`, `events`, `layout`, `theme`, `overlays`, `router`, `forms`) are enforced as cleanly separated modules first, and may only be split into standalone crates if an external consumer needs them independently of `nyrui-core`.

---

## 13. Testing Strategy

1. **Arena & Reconciliation Unit Tests (`nyrui-core`):**
   - Verify state preservation across prop updates, sibling insertions/removals, and keyed list permutations (`[A, B, C] -> [C, A, B]`).
   - Verify that unmounting a subtree detaches all `Signal` subscriptions, cancels scoped tasks, evacuates focus, and increments `NodeId` generations.
   - Verify that positional nodes in separate `if / else` branches do not alias state.
2. **Reactivity & Scheduler Tests:**
   - Verify diamond dependency graphs (`A -> B, A -> C, (B, C) -> D`) recompute `D` exactly once per batch.
   - Verify cycle detection panics deterministically.
3. **Compile-Fail & Macro Expansion Tests (`trybuild`):**
   - Verify readable diagnostics when required props are omitted, unknown props are supplied, or types mismatch inside `ui!`.
4. **Headless Buffer Snapshot Tests (`ratatui::backend::TestBackend`):**
   - Drive the full `Runtime` in headless mode using synthetic `UiEvent` sequences and assert the resulting `ratatui::buffer::Buffer` output via `insta` snapshots.

---

## 14. Implementation Roadmap

| Phase | Scope | Exit Criteria |
| ----- | ----- | ------------- |
| **Phase 1: Arena Runtime & Reconciler** | `NodeId`, `InstanceNode`, `Component` trait, `UiNode`, keyed/positional reconciliation, lifecycle hooks (`create`, `update`, `on_mount`, `on_unmount`). | Unit tests pass for mounting, updating props, reordering keyed lists, and unmounting while preserving instance state. |
| **Phase 2: Reactivity & Scheduler** | `Signal<T>`, `Computed<T>`, automatic dependency tracking during `render`, batched dirty-node min-heap scheduler. | Diamond reactive graph and selective subtree re-rendering verified without full-tree renders. |
| **Phase 3: Layout, Renderer & Events** | `Ratatui` renderer, layout pass with cached `Rect`s, `NativeWidget` escape hatch, Capture/Target/Bubble event dispatch, mouse hit-testing, Focus manager. | Interactive counter and multi-pane layout running on `TestBackend` and `CrosstermBackend`. |
| **Phase 4: `ui!` Macro & Props Derive** | `#[derive(Props)]`, `#[component]`, `ui!` proc-macro supporting props, slots, `if/else`, `match`, and `for` loops. | Complete builder parity and `trybuild` compile-fail test suite passing. |
| **Phase 5: Widgets, Theme, Overlays & Forms** | Semantic `Theme`, `OverlayManager` (modals, focus trap), `Router`, `FormState`, and core widget library (`nyrui-widgets`). | Reference application (**Nysh**, SSH identity manager) implemented end-to-end. |
