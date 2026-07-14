//! Static enforcement for the open product's offline and one-way dependency boundaries.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SKIPPED_DIRECTORIES: &[&str] = &[".git", "node_modules", "target"];
const NETWORK_PACKAGES: &[&str] = &[
    "async-tungstenite",
    "awc",
    "curl",
    "curl-sys",
    "h3",
    "hyper",
    "hyper-util",
    "isahc",
    "quinn",
    "reqwest",
    "surf",
    "tokio-tungstenite",
    "tonic",
    "tungstenite",
    "ureq",
    "websocket",
    "ws",
];
const NETWORK_MODULE_PREFIXES: &[&[&str]] = &[
    &["std", "net"],
    &["async_std", "net"],
    &["mio", "net"],
    &["smol", "net"],
    &["tokio", "net"],
    &["socket2"],
    &["libc", "socket"],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanReport {
    files_scanned: usize,
    manifests_scanned: usize,
}

impl ScanReport {
    #[must_use]
    pub const fn files_scanned(self) -> usize {
        self.files_scanned
    }

    #[must_use]
    pub const fn manifests_scanned(self) -> usize {
        self.manifests_scanned
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    code: &'static str,
    path: PathBuf,
    line: usize,
    message: String,
}

impl Violation {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violations(Vec<Violation>);

impl Violations {
    pub fn iter(&self) -> impl Iterator<Item = &Violation> {
        self.0.iter()
    }
}

impl fmt::Display for Violations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for violation in &self.0 {
            writeln!(
                formatter,
                "{}: {}:{}: {}",
                violation.code,
                violation.path.display(),
                violation.line,
                violation.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for Violations {}

/// Scans a Cargo-based open tree without building, fetching, or following symlinks.
pub fn scan_open_tree(root: &Path) -> Result<ScanReport, Violations> {
    let mut violations = Vec::new();
    if !root.is_dir() {
        push_violation(
            &mut violations,
            "root.missing",
            root,
            0,
            "open-tree root is not a directory",
        );
        return Err(Violations(violations));
    }

    let mut report = ScanReport {
        files_scanned: 0,
        manifests_scanned: 0,
    };
    walk(root, root, &mut report, &mut violations);
    if report.manifests_scanned == 0 {
        push_violation(
            &mut violations,
            "manifest.missing",
            root,
            0,
            "open tree contains no Cargo.toml manifests",
        );
    }

    violations.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.code.cmp(right.code))
            .then_with(|| left.message.cmp(&right.message))
    });
    violations.dedup();
    if violations.is_empty() {
        Ok(report)
    } else {
        Err(Violations(violations))
    }
}

fn walk(root: &Path, directory: &Path, report: &mut ScanReport, violations: &mut Vec<Violation>) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            push_io(violations, "path.read", root, directory, &error);
            return;
        }
    };
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            push_io(violations, "path.read", root, directory, &error);
            Vec::new()
        });
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                push_io(violations, "path.metadata", root, &path, &error);
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            push_violation(
                violations,
                "path.symlink",
                &relative(root, &path),
                0,
                "symlinks are forbidden because they can escape static boundary scanning",
            );
        } else if metadata.is_dir() {
            let name = entry.file_name();
            if !SKIPPED_DIRECTORIES
                .iter()
                .any(|skipped| name == std::ffi::OsStr::new(skipped))
            {
                walk(root, &path, report, violations);
            }
        } else if metadata.is_file() {
            scan_file(root, &path, report, violations);
        }
    }
}

fn scan_file(root: &Path, path: &Path, report: &mut ScanReport, violations: &mut Vec<Violation>) {
    let is_manifest = path.file_name().is_some_and(|name| name == "Cargo.toml");
    let is_rust = path.extension().is_some_and(|extension| extension == "rs");
    if !is_manifest && !is_rust {
        return;
    }
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            push_io(violations, "file.read", root, path, &error);
            return;
        }
    };
    report.files_scanned += 1;
    let display_path = relative(root, path);
    if is_manifest {
        report.manifests_scanned += 1;
        scan_manifest(&display_path, &source, violations);
    } else {
        scan_rust(&display_path, &source, violations);
    }
}

fn scan_manifest(path: &Path, source: &str, violations: &mut Vec<Violation>) {
    let mut dependency_section = false;
    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let header = line.trim_matches(['[', ']']).replace(' ', "");
            dependency_section = is_dependency_header(&header);
            if let Some(package) = dependency_subtable_package(&header) {
                check_network_package(path, line_number, package, violations);
                check_closed_package(path, line_number, package, violations);
            }
            continue;
        }
        if dependency_section {
            if let Some((name, _)) = line.split_once('=') {
                let name = trim_toml_key(name);
                if name != "package" {
                    check_network_package(path, line_number, name, violations);
                    check_closed_package(path, line_number, name, violations);
                }
            }
            if let Some(package) = assigned_string(line, "package") {
                check_network_package(path, line_number, package, violations);
                check_closed_package(path, line_number, package, violations);
            }
        }
        if let Some(value) = assigned_string(line, "path") {
            if has_closed_component(value) {
                push_violation(
                    violations,
                    "closed.reference",
                    path,
                    line_number,
                    "manifest path crosses into the closed product tree",
                );
            }
        }
    }
}

fn is_dependency_header(header: &str) -> bool {
    let components = header.split('.').collect::<Vec<_>>();
    components.iter().any(|component| {
        matches!(
            *component,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        )
    })
}

fn dependency_subtable_package(header: &str) -> Option<&str> {
    let components = header.split('.').collect::<Vec<_>>();
    let index = components.iter().position(|component| {
        matches!(
            *component,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        )
    })?;
    components.get(index + 1).copied().map(trim_toml_key)
}

fn trim_toml_key(key: &str) -> &str {
    key.trim().trim_matches(['\'', '"'])
}

fn assigned_string<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let mut remainder = line;
    while let Some(position) = remainder.find(key) {
        let candidate = &remainder[position..];
        let before_ok = position == 0
            || !remainder.as_bytes()[position - 1].is_ascii_alphanumeric()
                && remainder.as_bytes()[position - 1] != b'_';
        let after_key = &candidate[key.len()..];
        if before_ok {
            let after_equals = after_key.trim_start().strip_prefix('=')?.trim_start();
            let quote = after_equals.as_bytes().first().copied()?;
            if quote == b'"' || quote == b'\'' {
                let value = &after_equals[1..];
                let end = value.find(char::from(quote))?;
                return Some(&value[..end]);
            }
        }
        remainder = &candidate[key.len()..];
    }
    None
}

fn check_network_package(path: &Path, line: usize, package: &str, violations: &mut Vec<Violation>) {
    let normalized = package.trim().replace('_', "-").to_ascii_lowercase();
    if NETWORK_PACKAGES.contains(&normalized.as_str()) {
        push_violation(
            violations,
            "network.dependency",
            path,
            line,
            &format!("forbidden network client dependency `{package}`"),
        );
    }
}

fn check_closed_package(path: &Path, line: usize, package: &str, violations: &mut Vec<Violation>) {
    let normalized = package.trim().replace('_', "-").to_ascii_lowercase();
    if normalized == "closed"
        || normalized.starts_with("closed-")
        || normalized == "superi-max"
        || normalized.starts_with("superi-max-")
    {
        push_violation(
            violations,
            "closed.reference",
            path,
            line,
            &format!("open-tree dependency references closed package `{package}`"),
        );
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quote == Some('"') {
            escaped = true;
        } else if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
        } else if character == '#' && quote.is_none() {
            return &line[..index];
        }
    }
    line
}

#[derive(Debug)]
struct Token {
    kind: TokenKind,
    line: usize,
}

#[derive(Debug)]
enum TokenKind {
    Identifier(String),
    Literal(String),
    Punctuation(char),
}

fn scan_rust(path: &Path, source: &str, violations: &mut Vec<Violation>) {
    let tokens = lex_rust(source);
    let identifiers = identifier_chains(&tokens);
    for (line, chain) in identifiers {
        if NETWORK_MODULE_PREFIXES
            .iter()
            .any(|prefix| chain.starts_with(prefix))
        {
            push_violation(
                violations,
                "network.api",
                path,
                line,
                &format!("forbidden network API `{}`", chain.join("::")),
            );
        }
    }
    for (line, module) in nested_use_network_modules(&tokens) {
        push_violation(
            violations,
            "network.api",
            path,
            line,
            &format!("forbidden network API `{module}::{{net}}`"),
        );
    }

    for (index, token) in tokens.iter().enumerate() {
        let TokenKind::Identifier(name) = &token.kind else {
            continue;
        };
        if matches!(name.as_str(), "include" | "include_bytes" | "include_str")
            && token_is_punctuation(tokens.get(index + 1), '!')
        {
            for following in tokens.iter().skip(index + 2).take(12) {
                if let TokenKind::Literal(value) = &following.kind {
                    if has_closed_component(value) {
                        push_violation(
                            violations,
                            "closed.reference",
                            path,
                            token.line,
                            "source include crosses into the closed product tree",
                        );
                    }
                }
            }
        }
        if name == "path" && token_is_punctuation(tokens.get(index + 1), '=') {
            if let Some(Token {
                kind: TokenKind::Literal(value),
                ..
            }) = tokens.get(index + 2)
            {
                if has_closed_component(value) {
                    push_violation(
                        violations,
                        "closed.reference",
                        path,
                        token.line,
                        "module path crosses into the closed product tree",
                    );
                }
            }
        }
    }
}

fn nested_use_network_modules(tokens: &[Token]) -> Vec<(usize, &str)> {
    let mut modules = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let TokenKind::Identifier(module) = &token.kind else {
            continue;
        };
        if !matches!(
            module.as_str(),
            "std" | "async_std" | "mio" | "smol" | "tokio"
        ) {
            continue;
        }
        if token_is_punctuation(tokens.get(index + 1), ':')
            && token_is_punctuation(tokens.get(index + 2), ':')
            && token_is_punctuation(tokens.get(index + 3), '{')
            && matches!(
                tokens.get(index + 4),
                Some(Token {
                    kind: TokenKind::Identifier(member),
                    ..
                }) if member == "net"
            )
        {
            modules.push((token.line, module.as_str()));
        }
    }
    modules
}

fn identifier_chains(tokens: &[Token]) -> Vec<(usize, Vec<&str>)> {
    let mut chains = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let TokenKind::Identifier(first) = &tokens[index].kind else {
            index += 1;
            continue;
        };
        let mut chain = vec![first.as_str()];
        let line = tokens[index].line;
        let mut cursor = index + 1;
        while token_is_punctuation(tokens.get(cursor), ':')
            && token_is_punctuation(tokens.get(cursor + 1), ':')
        {
            let Some(Token {
                kind: TokenKind::Identifier(next),
                ..
            }) = tokens.get(cursor + 2)
            else {
                break;
            };
            chain.push(next);
            cursor += 3;
        }
        chains.push((line, chain));
        index = cursor.max(index + 1);
    }
    chains
}

fn token_is_punctuation(token: Option<&Token>, expected: char) -> bool {
    matches!(token, Some(Token { kind: TokenKind::Punctuation(actual), .. }) if *actual == expected)
}

fn lex_rust(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            line += 1;
            index += 1;
        } else if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index += 2;
            let mut depth = 1_u32;
            while index < bytes.len() && depth > 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    if bytes[index] == b'\n' {
                        line += 1;
                    }
                    index += 1;
                }
            }
        } else if let Some((literal, next, added_lines)) = rust_string(bytes, index) {
            tokens.push(Token {
                kind: TokenKind::Literal(literal),
                line,
            });
            line += added_lines;
            index = next;
        } else if is_identifier_start(bytes[index]) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_identifier_continue(bytes[index]) {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Identifier(String::from_utf8_lossy(&bytes[start..index]).into()),
                line,
            });
        } else if bytes[index] == b'\'' {
            if let Some(end) = char_literal_end(bytes, index) {
                index = end;
            } else {
                tokens.push(Token {
                    kind: TokenKind::Punctuation('\''),
                    line,
                });
                index += 1;
            }
        } else {
            tokens.push(Token {
                kind: TokenKind::Punctuation(char::from(bytes[index])),
                line,
            });
            index += 1;
        }
    }
    tokens
}

fn rust_string(bytes: &[u8], index: usize) -> Option<(String, usize, usize)> {
    let mut start = index;
    if bytes.get(start) == Some(&b'b') {
        start += 1;
    }
    if bytes.get(start) == Some(&b'"') {
        let mut cursor = start + 1;
        let value_start = cursor;
        let mut escaped = false;
        let mut lines = 0;
        while cursor < bytes.len() {
            if bytes[cursor] == b'\n' {
                lines += 1;
            }
            if escaped {
                escaped = false;
            } else if bytes[cursor] == b'\\' {
                escaped = true;
            } else if bytes[cursor] == b'"' {
                return Some((
                    String::from_utf8_lossy(&bytes[value_start..cursor]).into(),
                    cursor + 1,
                    lines,
                ));
            }
            cursor += 1;
        }
        return Some((String::new(), bytes.len(), lines));
    }
    if bytes.get(start) != Some(&b'r') {
        return None;
    }
    let mut cursor = start + 1;
    let mut hashes = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    cursor += 1;
    let value_start = cursor;
    let mut lines = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\n' {
            lines += 1;
        }
        if bytes[cursor] == b'"'
            && bytes
                .get(cursor + 1..cursor + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return Some((
                String::from_utf8_lossy(&bytes[value_start..cursor]).into(),
                cursor + 1 + hashes,
                lines,
            ));
        }
        cursor += 1;
    }
    Some((String::new(), bytes.len(), lines))
}

fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let first = *bytes.get(start + 1)?;
    if first == b'\\' {
        let mut index = start + 2;
        while index < bytes.len() && index <= start + 14 && bytes[index] != b'\n' {
            if bytes[index] == b'\'' {
                return Some(index + 1);
            }
            index += 1;
        }
        return None;
    }
    let width = if first < 0x80 {
        1
    } else if first & 0b1110_0000 == 0b1100_0000 {
        2
    } else if first & 0b1111_0000 == 0b1110_0000 {
        3
    } else if first & 0b1111_1000 == 0b1111_0000 {
        4
    } else {
        return None;
    };
    (bytes.get(start + 1 + width) == Some(&b'\'')).then_some(start + 2 + width)
}

const fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

const fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn has_closed_component(value: &str) -> bool {
    value
        .split(['/', '\\'])
        .any(|component| component.eq_ignore_ascii_case("closed"))
}

fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn push_io(
    violations: &mut Vec<Violation>,
    code: &'static str,
    root: &Path,
    path: &Path,
    error: &io::Error,
) {
    push_violation(
        violations,
        code,
        &relative(root, path),
        0,
        &error.to_string(),
    );
}

fn push_violation(
    violations: &mut Vec<Violation>,
    code: &'static str,
    path: &Path,
    line: usize,
    message: &str,
) {
    violations.push(Violation {
        code,
        path: path.to_path_buf(),
        line,
        message: message.to_owned(),
    });
}
