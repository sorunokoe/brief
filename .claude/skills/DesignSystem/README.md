# DesignSystem

TdsCompose design system for GolfApp Android. All UI components must use these primitives — no custom spacing or colour values.

## Interface

```
fn scaffold(topBar: String, content: String) -> String
fn navigationHeader(title: String, onBack: String) -> String
fn listItem(headline: String, trailing: String, onClick: String) -> String
fn radioButton(selected: String) -> String
fn themeToken(name: String) -> String
```

## Notes

- `scaffold(topBar, content)` — TdsScaffold with top bar and content slots
- `navigationHeader(title, onBack)` — TdsNavigationHeader with back button
- `listItem(headline, trailing, onClick)` — TdsListItem with configurable trailing content
- `radioButton(selected)` — TdsListItemTrailingContent.RadioButton
- `themeToken(name)` — access TdsTheme spacing/colour/typography tokens
