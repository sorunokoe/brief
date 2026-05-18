# GolfPreferences

GolfApp local preferences backed by multiplatform-settings. Provides reactive (Flow-based) read and coroutine-based write access to user preferences.

## Interface

```
fn readLanguage() -> String
fn writeLanguage(language: String) -> String
```

## Notes

- `readLanguage()` — returns the current language preference as a Flow
- `writeLanguage(language)` — persists a new language selection; suspend function
