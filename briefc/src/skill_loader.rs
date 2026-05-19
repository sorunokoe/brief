use std::collections::HashSet;
use std::fs;
use std::path::Path;

const EXPECTED_ABI_VERSION: &str = "1.0";
const MISSING_ABI_VERSION: &str = "<missing>";
const ABI_PRIMITIVES: [&str; 3] = ["String", "Int", "Bool"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDecl {
    pub name: String,
    pub abi_version: String,
    pub functions: Vec<SkillFn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFn {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillLoadError {
    FileNotFound(String),
    AbiVersionMismatch { expected: &'static str, found: String },
    UnknownType { fn_name: String, type_name: String },
    ParseError(String),
}

pub fn requires_abi_verification(content: &str) -> bool {
    content.lines().map(str::trim).any(|line| {
        line.starts_with("// abi_version:")
            || line.starts_with("opaque type ")
            || line.starts_with("sealed type ")
    })
}

/// Parse and validate a .briefskill interface file.
/// `skill_path`: path to the .briefskill file
/// `known_types`: set of known opaque + sealed type names from the importing Brief program
pub fn load_skill(
    skill_path: &Path,
    known_types: &HashSet<String>,
) -> Result<SkillDecl, SkillLoadError> {
    let content = fs::read_to_string(skill_path)
        .map_err(|_| SkillLoadError::FileNotFound(skill_path.display().to_string()))?;

    let abi_version = extract_abi_version(&content).ok_or_else(|| SkillLoadError::AbiVersionMismatch {
        expected: EXPECTED_ABI_VERSION,
        found: MISSING_ABI_VERSION.to_string(),
    })?;
    if !is_compatible_abi_version(&abi_version) {
        return Err(SkillLoadError::AbiVersionMismatch {
            expected: EXPECTED_ABI_VERSION,
            found: abi_version,
        });
    }

    let mut allowed_types = known_types.clone();
    for primitive in ABI_PRIMITIVES {
        allowed_types.insert(primitive.to_string());
    }

    let mut interface_name: Option<String> = None;
    let mut functions = Vec::new();
    let mut sealed_field_types = Vec::new();
    let mut in_interface = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if !in_interface {
            if let Some(name) = parse_interface_start(line) {
                interface_name = Some(name);
                in_interface = true;
            }
            continue;
        }

        if line == "}" {
            in_interface = false;
            continue;
        }

        if let Some(name) = parse_opaque_type_decl(line) {
            allowed_types.insert(name);
            continue;
        }

        if let Some((name, field_types)) = parse_sealed_type_decl(line)? {
            allowed_types.insert(name);
            sealed_field_types.extend(field_types);
            continue;
        }

        if line.starts_with("fn ") {
            functions.push(parse_fn_decl(line)?);
        }
    }

    let name = interface_name.ok_or_else(|| SkillLoadError::ParseError("missing interface block".to_string()))?;

    for field_ty in sealed_field_types {
        let Some(type_name) = first_unknown_type_name(&field_ty, &allowed_types) else {
            continue;
        };
        return Err(SkillLoadError::ParseError(format!(
            "sealed type in skill '{}' references unknown type '{}'",
            name, type_name
        )));
    }

    for function in &functions {
        for (_, ty) in &function.params {
            if let Some(type_name) = first_unknown_type_name(ty, &allowed_types) {
                return Err(SkillLoadError::UnknownType {
                    fn_name: function.name.clone(),
                    type_name,
                });
            }
        }
        if let Some(type_name) = first_unknown_type_name(&function.return_type, &allowed_types) {
            return Err(SkillLoadError::UnknownType {
                fn_name: function.name.clone(),
                type_name,
            });
        }
    }

    Ok(SkillDecl {
        name,
        abi_version,
        functions,
    })
}

fn extract_abi_version(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix("// abi_version:")?.trim();
        Some(value.trim_matches('"').to_string())
    })
}

fn is_compatible_abi_version(version: &str) -> bool {
    version == EXPECTED_ABI_VERSION || version.starts_with("1.")
}

fn parse_interface_start(line: &str) -> Option<String> {
    let rest = line.strip_prefix("interface ")?;
    let name = rest.strip_suffix('{')?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_opaque_type_decl(line: &str) -> Option<String> {
    line.strip_prefix("opaque type ")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
}

fn parse_sealed_type_decl(line: &str) -> Result<Option<(String, Vec<String>)>, SkillLoadError> {
    let Some(rest) = line.strip_prefix("sealed type ") else {
        return Ok(None);
    };
    let (name, variants) = rest
        .split_once('=')
        .ok_or_else(|| SkillLoadError::ParseError(format!("invalid sealed type declaration: {line}")))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(SkillLoadError::ParseError(format!(
            "invalid sealed type declaration: {line}"
        )));
    }

    let mut field_types = Vec::new();
    for variant in split_top_level(variants.trim(), '|') {
        let variant = variant.trim();
        if variant.is_empty() {
            continue;
        }
        if let Some(open) = variant.find('(') {
            let close = matching_delimiter(variant, open, '(', ')').ok_or_else(|| {
                SkillLoadError::ParseError(format!("invalid sealed type variant: {variant}"))
            })?;
            let fields = &variant[open + 1..close];
            for field in split_top_level(fields, ',') {
                let field = field.trim();
                if !field.is_empty() {
                    field_types.push(field.to_string());
                }
            }
        }
    }

    Ok(Some((name.to_string(), field_types)))
}

fn parse_fn_decl(line: &str) -> Result<SkillFn, SkillLoadError> {
    let rest = line
        .strip_prefix("fn ")
        .ok_or_else(|| SkillLoadError::ParseError(format!("invalid fn declaration: {line}")))?;
    let open = rest
        .find('(')
        .ok_or_else(|| SkillLoadError::ParseError(format!("invalid fn declaration: {line}")))?;
    let close = matching_delimiter(rest, open, '(', ')')
        .ok_or_else(|| SkillLoadError::ParseError(format!("invalid fn declaration: {line}")))?;

    let name = rest[..open].trim();
    if name.is_empty() {
        return Err(SkillLoadError::ParseError(format!("invalid fn declaration: {line}")));
    }

    let params = split_params(&rest[open + 1..close])?;
    let tail = rest[close + 1..].trim();
    let return_type = tail
        .strip_prefix("->")
        .map(str::trim)
        .filter(|ty| !ty.is_empty())
        .ok_or_else(|| SkillLoadError::ParseError(format!("invalid fn declaration: {line}")))?;

    Ok(SkillFn {
        name: name.to_string(),
        params,
        return_type: return_type.to_string(),
    })
}

fn split_params(input: &str) -> Result<Vec<(String, String)>, SkillLoadError> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    split_top_level(input, ',')
        .into_iter()
        .map(|param| parse_param_decl(param.trim()))
        .collect()
}

fn parse_param_decl(param: &str) -> Result<(String, String), SkillLoadError> {
    let (name, ty) = param
        .split_once(':')
        .ok_or_else(|| SkillLoadError::ParseError(format!("invalid parameter declaration: {param}")))?;
    let name = name.trim();
    let ty = ty.trim();
    if name.is_empty() || ty.is_empty() {
        return Err(SkillLoadError::ParseError(format!(
            "invalid parameter declaration: {param}"
        )));
    }
    Ok((name.to_string(), ty.to_string()))
}

fn first_unknown_type_name(type_str: &str, allowed_types: &HashSet<String>) -> Option<String> {
    let stripped = strip_leading_annotations(type_str.trim());
    for token in extract_identifiers(stripped) {
        if !allowed_types.contains(&token) {
            return Some(token);
        }
    }
    None
}

fn strip_leading_annotations(mut value: &str) -> &str {
    loop {
        value = value.trim_start();
        if !value.starts_with('@') {
            return value;
        }

        let mut depth = 0usize;
        let mut end = 0usize;
        let mut saw_space = false;
        for (idx, ch) in value.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                c if c.is_whitespace() && depth == 0 => {
                    saw_space = true;
                    end = idx + c.len_utf8();
                    break;
                }
                _ => {}
            }
            end = idx + ch.len_utf8();
        }

        if !saw_space {
            return "";
        }
        value = &value[end..];
    }
}

fn extract_identifiers(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            current.push(ch);
        } else if !current.is_empty() {
            names.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        names.push(current);
    }

    names
}

fn split_top_level(input: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren = 0usize;
    let mut angle = 0usize;
    let mut bracket = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' if paren > 0 => paren -= 1,
            '<' => angle += 1,
            '>' if angle > 0 => angle -= 1,
            '[' => bracket += 1,
            ']' if bracket > 0 => bracket -= 1,
            _ => {}
        }

        if ch == delimiter && paren == 0 && angle == 0 && bracket == 0 {
            parts.push(&input[start..idx]);
            start = idx + ch.len_utf8();
        }
    }

    parts.push(&input[start..]);
    parts
}

fn matching_delimiter(input: &str, open_idx: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in input.char_indices().skip_while(|(idx, _)| *idx < open_idx) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    fn write_skill(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().expect("named temp file");
        fs::write(file.path(), content).expect("write skill");
        file
    }

    fn known_types() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn test_load_valid_skill() {
        let file = write_skill(
            r#"// abi_version: "1.0"
interface LexerPrimitives {
    opaque type TokenStream
    opaque type DiagnosticSet
    sealed type LexOutcome = Lexed(TokenStream) | LexFailed(DiagnosticSet)
    fn tokenize(source: String, filePath: String) -> LexOutcome
    fn classify(tokens: TokenStream) -> LexOutcome
}
"#,
        );

        let decl = load_skill(file.path(), &known_types()).expect("valid skill should load");
        assert_eq!(decl.name, "LexerPrimitives");
        assert_eq!(decl.abi_version, "1.0");
        assert_eq!(decl.functions.len(), 2);
        assert_eq!(decl.functions[0].name, "tokenize");
        assert_eq!(
            decl.functions[0].params,
            vec![
                ("source".to_string(), "String".to_string()),
                ("filePath".to_string(), "String".to_string())
            ]
        );
        assert_eq!(decl.functions[0].return_type, "LexOutcome");
    }

    #[test]
    fn test_load_abi_version_mismatch() {
        let file = write_skill(
            r#"// abi_version: "2.0"
interface Broken {
    fn ping() -> String
}
"#,
        );

        assert_eq!(
            load_skill(file.path(), &known_types()),
            Err(SkillLoadError::AbiVersionMismatch {
                expected: EXPECTED_ABI_VERSION,
                found: "2.0".to_string(),
            })
        );
    }

    #[test]
    fn test_load_missing_abi_version() {
        let file = write_skill(
            r#"interface Broken {
    opaque type TokenStream
    fn ping() -> TokenStream
}
"#,
        );

        assert_eq!(
            load_skill(file.path(), &known_types()),
            Err(SkillLoadError::AbiVersionMismatch {
                expected: EXPECTED_ABI_VERSION,
                found: MISSING_ABI_VERSION.to_string(),
            })
        );
    }

    #[test]
    fn test_load_unknown_type() {
        let file = write_skill(
            r#"// abi_version: "1.0"
interface Broken {
    fn ping() -> UnknownType
}
"#,
        );

        assert_eq!(
            load_skill(file.path(), &known_types()),
            Err(SkillLoadError::UnknownType {
                fn_name: "ping".to_string(),
                type_name: "UnknownType".to_string(),
            })
        );
    }
}
