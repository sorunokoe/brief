# CircuitUDF

Circuit UDF (Unidirectional Data Flow) for GolfApp Android. Implements the Slack Circuit pattern: Screen → Presenter → UiState → Composable UI.

## Interface

```
fn createPresenter(screen: String, navigator: String) -> String
fn emitUiState(state: String) -> String
fn sendEvent(event: String) -> String
fn collectRetainedState(flow: String) -> String
```

## Notes

- `createPresenter(screen, navigator)` — creates a Presenter bound to a Screen
- `emitUiState(state)` — emits immutable UiState from Presenter
- `sendEvent(event)` — fires a CircuitUiEvent from the UI to the Presenter
- `collectRetainedState(flow)` — collects a Flow as retained Compose state (survives configuration changes)
