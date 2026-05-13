# Sealed Types

Sealed types are algebraic sum types — exactly one of a set of named variants.

## Syntax

```brief
sealed Platform {
    iOS     { osVersion: String }
    Android { sdkVersion: Int }
    Web     { browser: String }
}
```

## Usage

```brief
effect Builder {
    fn build(target: Platform) -> Artifact
}
```

See `examples/29-platform-branching.brief` for a full platform-branching example.
