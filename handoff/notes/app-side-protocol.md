# App-side protocol notes (from MCP builder M1) — for the UI builder's second pass

The engine and MCP sides are on branch `integ-m1` (commits ac26bb33, 16beecc3, 43b99acc); they are merged
into the main branch before this pass starts.

1. **`errorDetail`.** `control::handle` returns `Result<Value, String>` and many tests call it, so keep the
   signature. Store the last error's detail on the app (as the MCP backends do) wherever an `EngineError` is
   stringified: the `engine.execute` arm, `in_document` / `bring_forward`, `node.get`, `ui.render`, `ui.type`.
   Add `detail: Option<Value>` to `ControlReply` (`crates/ui-egui/src/control.rs:43`, built near lines 583
   and 792–794). Write it in `apps/figcraft/src/control_server.rs` (~lines 192 and 216):
   `{"id":…,"ok":false,"error":"<text>","errorDetail":{"code":"noSuchNode","message":"<same text>","node":41,…}}`.
   Use `EngineError::detail()`; for handler-made strings use
   `{"code":"badParams"|"unknownMethod"|"failed","message"}`.
2. **`app.info {}`** → `{"instance":"<random id made once at app start>","version":"<CARGO_PKG_VERSION>",
   "pid":<u32>,"window":true,"features":["errorDetail","revision"]}`. The MCP side reads `instance` and
   `features` (the `errorDetail` entry sets `engineErrorCodes`).
3. **`agent` on `ui.pointer`, `ui.key`, `ui.type`.** The MCP server already sends it; wrap those handlers in
   `app.session.acting_as(agent, …)` so gestures are the agent's undo step. Today the app records them as the
   user's.
4. **Dialog-only commands.** The `engine.execute` arm drops `dialog.*` requests and still answers plain
   success. Add `"ui": "dropped (…)"` to object results, as headless does.
5. The app must not set `Session::windowless`.
6. Behaviour the UI will see from the engine: `file.new` / `open` / `duplicate` / `revert` results gain `id`
   and `revision`; `doc.list` gains `revision`; `doc.nextTab` counts from the visible front; `edit.undo` /
   `redo` take `others`; a failed batch step's text is `step N (command): …`; `proto.interaction.add` / `.set`
   validate actions.
