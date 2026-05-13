# Structs & Protocols

## Structs

```brief
struct UserProfile {
    @nonempty displayName: String
    @url      avatarUrl:   String
    @matches("[^@]+@[^@]+") email: String
}
```

## Protocols

```brief
protocol Repository {
    fn findById(id: String) -> User
    fn save(user: User) -> Unit
}
```

Protocols are abstract interfaces used in effect signatures and as type annotations.
