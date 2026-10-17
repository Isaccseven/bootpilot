//! Conservative editor for the static, flat Grub2Win configuration shape.
//! Unknown code is retained byte-for-byte; unsupported global execution is read-only.
use crate::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ops::Range;

const START: &str = "# start-grub2win-user-section";
const END: &str = "# end-grub2win-user-section";
const TRUSTED_SOURCE: &str = "source $prefix/g2bootmgr/gnugrub.functions.cfg";
const TRUSTED_THEME: &str = "if [ $grub2win_bootmode = EFI ] ; then set theme=$theme.$grub2win_efibits.efi.txt ; else set theme=$theme.$grub2win_procbits.bios.txt ; fi";

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Word,
    Open,
    Close,
    Newline,
    Semi,
}
#[derive(Clone, Debug)]
struct Token {
    kind: Kind,
    value: String,
    span: Range<usize>,
    literal: bool,
}
#[derive(Clone, Debug)]
struct Setting {
    name: String,
    value: String,
    span: Range<usize>,
}
#[derive(Default)]
struct Analysis {
    entries: Vec<MenuEntry>,
    references: Vec<String>,
    settings: Vec<Setting>,
    user_span: Option<Range<usize>>,
    reason: Option<String>,
}
impl Analysis {
    fn block(&mut self, reason: &str) {
        if self.reason.is_none() {
            self.reason = Some(reason.into());
        }
    }
}

pub fn parse_grub(main: &[u8], user: &[u8]) -> BootResult<GrubDocument> {
    let analysis = analyze(main, user);
    let default = analysis
        .settings
        .iter()
        .rev()
        .find(|s| s.name == "default")
        .and_then(|s| resolve_default(&analysis, &s.value));
    let timeout = analysis
        .settings
        .iter()
        .rev()
        .find(|s| s.name == "timeout")
        .and_then(|s| s.value.parse().ok());
    Ok(GrubDocument {
        main_bytes: main.to_vec(),
        user_bytes: user.to_vec(),
        entries: analysis.entries,
        default_entry_id: default,
        timeout_seconds: timeout,
        write_supported: analysis.reason.is_none(),
        write_block_reason: analysis.reason,
    })
}

fn resolve_default(a: &Analysis, value: &str) -> Option<String> {
    if let Ok(index) = value.parse::<usize>() {
        if index.to_string() != value {
            return None;
        }
        return a.entries.get(index).map(|e| e.id.clone());
    }
    a.entries
        .iter()
        .zip(&a.references)
        .find(|(_, r)| r.as_str() == value)
        .map(|(e, _)| e.id.clone())
}

fn analyze(main: &[u8], user: &[u8]) -> Analysis {
    let mut a = Analysis::default();
    let Ok(text) = std::str::from_utf8(main) else {
        a.block("Die Hauptdatei ist kein unterstütztes UTF-8-Dokument.");
        return a;
    };
    if std::str::from_utf8(user).is_err() {
        a.block("Die Benutzersektion ist kein unterstütztes UTF-8-Dokument.");
    }
    a.user_span = marker_span(text);
    match &a.user_span {
        Some(span) if &main[span.clone()] == user => {}
        _ => a.block("Die Grub2Win-Benutzersektion fehlt, ist mehrdeutig oder stimmt nicht bytegenau überein."),
    }
    let tokens = match tokenize(text) {
        Ok(t) => t,
        Err(()) => {
            a.block(
                "Die GRUB-Syntax enthält nicht unterstützte oder unvollständige Zeichenfolgen.",
            );
            return a;
        }
    };
    let mut i = 0;
    let mut ids = BTreeSet::new();
    let mut source_seen = false;
    let mut bits_seen = false;
    while i < tokens.len() {
        if tokens[i].kind == Kind::Newline {
            i += 1;
            continue;
        }
        let start = i;
        while i < tokens.len() && tokens[i].kind == Kind::Word {
            i += 1;
        }
        let words = &tokens[start..i];
        let command = words.first().map(|t| t.value.as_str()).unwrap_or("");
        if command == "menuentry" && i < tokens.len() && tokens[i].kind == Kind::Open {
            let open = i;
            let mut depth = 1;
            i += 1;
            while i < tokens.len() && depth > 0 {
                match tokens[i].kind {
                    Kind::Open => depth += 1,
                    Kind::Close => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 {
                a.block("Ein Menüblock ist nicht geschlossen.");
                break;
            }
            let block_end = tokens[i - 1].span.end;
            if let Some(span) = &a.user_span {
                let block_start = tokens[start].span.start;
                if (block_start < span.start && block_end > span.start)
                    || (block_start < span.end && block_end > span.end)
                {
                    a.block("Die Benutzersektion schneidet einen Menüblock.");
                }
            }
            match menu_header(words, &main[tokens[start].span.start..block_end], a.entries.len()) {
                Some((entry, reference)) => {
                    if !ids.insert(entry.id.clone()) {
                        a.block("Menü-IDs sind nicht eindeutig.");
                    }
                    a.entries.push(entry);
                    a.references.push(reference);
                }
                None => a.block("Ein Menüeintrag hat keinen eindeutig statischen Titel oder keine unterstützten Optionen."),
            }
            // Dynamic menu construction inside another menu is never treated as a flat entry.
            if tokens[open + 1..i - 1].iter().any(|t| {
                t.kind == Kind::Word && matches!(t.value.as_str(), "menuentry" | "submenu")
            }) {
                a.block("Verschachtelte Menüeinträge werden nur gelesen.");
            }
            if i < tokens.len() && tokens[i].kind != Kind::Newline {
                a.block("Code hinter einem Menüblock ist nicht unterstützt.");
            }
            continue;
        }
        // Only the generator's fixed theme conditional is supported outside menu blocks.
        let line_start = text[..tokens[start].span.start]
            .rfind('\n')
            .map_or(0, |p| p + 1);
        let line_end = text[tokens[start].span.start..]
            .find('\n')
            .map_or(text.len(), |p| tokens[start].span.start + p);
        let line = text[line_start..line_end]
            .trim()
            .trim_start_matches('\u{feff}');
        if line == TRUSTED_THEME && source_seen && !bits_seen {
            a.block("Die Grub2Win-Initialisierung ist nicht vollständig.");
        }
        if line == TRUSTED_THEME && source_seen && bits_seen && a.entries.is_empty() {
            while i < tokens.len() && tokens[i].kind != Kind::Newline {
                i += 1;
            }
            continue;
        }
        let standalone = i == tokens.len() || tokens[i].kind == Kind::Newline;
        if !standalone {
            a.block("Globale Befehle müssen eindeutig und einzeln auf einer Zeile stehen.");
        }
        match command {
            "set" if words.len() == 2 && standalone => {
                if let Some((name, value)) = words[1].value.split_once('=') {
                    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        || name.is_empty()
                    {
                        a.block("Dynamische Variablennamen sind nicht unterstützt.");
                    }
                    if matches!(name, "default" | "timeout") {
                        let valid = words[1].literal
                            && if name == "timeout" {
                                value.parse::<u16>().is_ok()
                            } else {
                                safe_id(value) || value.parse::<usize>().is_ok()
                            };
                        if valid {
                            a.settings.push(Setting {
                                name: name.into(),
                                value: value.into(),
                                span: words[1].span.clone(),
                            });
                        } else {
                            a.block("Default oder Wartezeit ist dynamisch oder nicht unterstützt.");
                        }
                    } else if !supported_assignment(name, value, words[1].literal) {
                        a.block("Die globale Zuweisung gehört nicht zur unterstützten Grub2Win-Vorlage.");
                    }
                } else {
                    a.block("Nicht unterstützte globale Zuweisung.");
                }
            }
            "source"
                if line == TRUSTED_SOURCE && standalone && !source_seen && a.entries.is_empty() =>
            {
                source_seen = true
            }
            "g2wbits"
                if words.len() == 1
                    && standalone
                    && source_seen
                    && !bits_seen
                    && a.entries.is_empty() =>
            {
                bits_seen = true
            }
            "export"
                if standalone
                    && words.len() == 2
                    && matches!(words[1].value.as_str(), "theme" | "icondir") => {}
            "insmod"
                if standalone
                    && words.len() == 2
                    && words[1].literal
                    && matches!(words[1].value.as_str(), "png" | "all_video" | "gfxmenu") => {}
            "loadfont" if standalone && line == "loadfont $prefix/fonts/unicode.pf2" => {}
            "terminal_output" if standalone && line == "terminal_output gfxterm" => {}
            _ => a.block(
                "Unbekannter globaler Code kann Menü oder Einstellungen dynamisch verändern.",
            ),
        }
        // Always progress past unsupported syntax, retaining the original bytes for display.
        while i < tokens.len() && tokens[i].kind != Kind::Newline {
            i += 1;
        }
    }
    if source_seen != bits_seen {
        a.block("Die Grub2Win-Initialisierung ist nicht vollständig.");
    }
    for name in ["default", "timeout"] {
        if !a.settings.iter().any(|s| s.name == name) {
            a.block("Eine eindeutige Default- oder Wartezeit-Zuweisung fehlt.");
        }
    }
    if a.entries.is_empty() {
        a.block("Keine statischen Menüeinträge erkannt.");
    }
    if let Some(s) = a.settings.iter().rev().find(|s| s.name == "default")
        && resolve_default(&a, &s.value).is_none()
    {
        a.block("Der konfigurierte Standard verweist auf keinen eindeutigen Menüeintrag.");
    }
    a
}

fn marker_span(text: &str) -> Option<Range<usize>> {
    let mut begin = None;
    let mut end = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        for (marker, is_start) in [(START, true), (END, false)] {
            if let Some(suffix) = trimmed.strip_prefix(marker) {
                if !suffix.chars().all(|c| c == '*' || c.is_ascii_whitespace()) {
                    return None;
                }
                if is_start {
                    if begin.is_some() || end.is_some() {
                        return None;
                    }
                    begin = Some(offset + line.len());
                } else {
                    if end.is_some() || begin.is_none() {
                        return None;
                    }
                    end = Some(offset);
                }
            }
        }
        offset += line.len();
    }
    match (begin, end) {
        (Some(b), Some(e)) if b <= e => Some(b..e),
        _ => None,
    }
}

fn menu_header(words: &[Token], block: &[u8], index: usize) -> Option<(MenuEntry, String)> {
    let title = words.get(1)?;
    if !title.literal || title.value.is_empty() || title.value.chars().any(char::is_control) {
        return None;
    }
    let mut id = None;
    let mut i = 2;
    while i < words.len() {
        let word = &words[i];
        if !word.literal {
            return None;
        }
        if word.value == "--unrestricted" {
            i += 1;
            continue;
        }
        let (key, value) = if let Some((k, v)) = word.value.split_once('=') {
            (k, v)
        } else {
            i += 1;
            let next = words.get(i)?;
            if !next.literal {
                return None;
            }
            (word.value.as_str(), next.value.as_str())
        };
        match key {
            "--id"
                if id.is_none()
                    && safe_id(value)
                    && !value.as_bytes()[0].is_ascii_digit()
                    && !value.starts_with("bootpilot-auto-") =>
            {
                id = Some(value.to_string())
            }
            "--class" | "--hotkey" if safe_id(value) => {}
            _ => return None,
        }
        i += 1;
    }
    let reference = id.clone().unwrap_or_else(|| index.to_string());
    let id = id.unwrap_or_else(|| format!("bootpilot-auto-{:x}", Sha256::digest(block)));
    let lower = title.value.to_lowercase();
    let kind = if lower.contains("windows") {
        MenuEntryKind::Windows
    } else if lower.contains("installer") {
        MenuEntryKind::Installer
    } else if lower.contains("linux") || lower.contains("cachyos") {
        MenuEntryKind::Linux
    } else {
        MenuEntryKind::Other
    };
    Some((
        MenuEntry {
            id,
            title: title.value.clone(),
            kind,
        },
        reference,
    ))
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn supported_assignment(name: &str, value: &str, literal: bool) -> bool {
    match name {
        "pager" => literal && matches!(value, "0" | "1"),
        "reviewpause" => literal && value.parse::<u16>().is_ok(),
        "icondir" => value == "$prefix/themes/icons",
        "locale_dir" => value == "$prefix/locale",
        "grub2win_langauto" => literal && matches!(value, "yes" | "no"),
        "gfxmode" => {
            literal
                && !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b',')
        }
        "gfxpayload" => literal && matches!(value, "text" | "keep"),
        "grub2win_efilevel" => literal && value.is_empty(),
        "theme" => value.strip_prefix("$prefix/themes/").is_some_and(safe_id),
        "timeout_style" => literal && matches!(value, "menu" | "countdown" | "hidden"),
        _ => false,
    }
}

// A small lexer, not an interpreter: track quotes/escapes and variable braces so literal
// braces in titles, comments and ${variables} never change menu nesting or offsets.
fn tokenize(text: &str) -> Result<Vec<Token>, ()> {
    let b = text.as_bytes();
    let mut i = if b.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut out = vec![];
    while i < b.len() {
        if b[i] == b' ' || b[i] == b'\t' || b[i] == b'\r' {
            i += 1;
            continue;
        }
        if b[i] == b'#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        let kind = match b[i] {
            b'\n' => Some(Kind::Newline),
            b';' => Some(Kind::Semi),
            b'{' => Some(Kind::Open),
            b'}' => Some(Kind::Close),
            _ => None,
        };
        if let Some(kind) = kind {
            i += 1;
            out.push(Token {
                kind,
                value: text[start..i].into(),
                span: start..i,
                literal: true,
            });
            continue;
        }
        let mut value = Vec::new();
        let mut quote = None;
        let mut literal = true;
        while i < b.len() {
            let c = b[i];
            if quote.is_none() && (c.is_ascii_whitespace() || b";{}".contains(&c)) {
                break;
            }
            if c == 0 || (c.is_ascii_control() && c != b'\t') {
                return Err(());
            }
            if c == b'\'' && quote != Some(b'"') {
                if quote == Some(c) {
                    quote = None
                } else {
                    quote = Some(c)
                }
                i += 1;
                continue;
            }
            if c == b'"' && quote != Some(b'\'') {
                if quote == Some(c) {
                    quote = None
                } else {
                    quote = Some(c)
                }
                i += 1;
                continue;
            }
            if c == b'\\' && quote != Some(b'\'') {
                i += 1;
                if i >= b.len() || b[i] == b'\n' || b[i] == b'\r' {
                    return Err(());
                }
                if quote == Some(b'"') && !b"\\\"$".contains(&b[i]) {
                    return Err(());
                }
                value.push(b[i]);
                i += 1;
                continue;
            }
            if quote != Some(b'\'') && (c == b'$' || c == b'`') {
                literal = false;
                if c == b'$' && b.get(i + 1) == Some(&b'{') {
                    let end = b[i + 2..]
                        .iter()
                        .position(|c| *c == b'}')
                        .map(|p| i + 2 + p)
                        .ok_or(())?;
                    value.extend_from_slice(&b[i..=end]);
                    i = end + 1;
                    continue;
                }
            }
            value.push(c);
            i += 1;
        }
        if quote.is_some() || i == start {
            return Err(());
        }
        out.push(Token {
            kind: Kind::Word,
            value: String::from_utf8(value).map_err(|_| ())?,
            span: start..i,
            literal,
        });
    }
    Ok(out)
}

pub(crate) fn edit_menu(
    doc: &GrubDocument,
    id: &str,
    timeout: u16,
) -> BootResult<(Vec<u8>, Vec<u8>)> {
    let a = checked(doc)?;
    let position = a.entries.iter().position(|e| e.id == id).ok_or_else(|| {
        BootError::new(
            ErrorCode::UnknownEntry,
            "Der Menüeintrag wurde nicht erkannt.",
        )
    })?;
    let mut changes = vec![];
    for setting in &a.settings {
        let value = if setting.name == "default" {
            a.references[position].clone()
        } else {
            timeout.to_string()
        };
        changes.push((
            setting.span.clone(),
            format!("{}={value}", setting.name).into_bytes(),
        ));
    }
    let main = replace_spans(&doc.main_bytes, changes);
    let text = std::str::from_utf8(&main).expect("checked UTF-8");
    let span = marker_span(text).expect("checked marker");
    let user = main[span].to_vec();
    ensure_output(main, user)
}

pub(crate) fn append_entry(doc: &GrubDocument, entry: &str) -> BootResult<(Vec<u8>, Vec<u8>)> {
    let a = checked(doc)?;
    let mut user = doc.user_bytes.clone();
    let newline = if user.windows(2).any(|w| w == b"\r\n")
        || doc.main_bytes.windows(2).any(|w| w == b"\r\n")
    {
        "\r\n"
    } else {
        "\n"
    };
    if !user.is_empty() && !user.ends_with(b"\n") {
        user.extend_from_slice(newline.as_bytes());
    }
    user.extend_from_slice(entry.replace('\n', newline).as_bytes());
    let main = replace_spans(&doc.main_bytes, vec![(a.user_span.unwrap(), user.clone())]);
    ensure_output(main, user)
}

fn checked(doc: &GrubDocument) -> BootResult<Analysis> {
    let a = analyze(&doc.main_bytes, &doc.user_bytes);
    if let Some(reason) = &a.reason {
        Err(BootError::new(ErrorCode::UnsupportedConfiguration, reason))
    } else {
        Ok(a)
    }
}
fn ensure_output(main: Vec<u8>, user: Vec<u8>) -> BootResult<(Vec<u8>, Vec<u8>)> {
    if let Some(reason) = analyze(&main, &user).reason {
        return Err(BootError::new(ErrorCode::UnsupportedConfiguration, reason));
    }
    Ok((main, user))
}
fn replace_spans(bytes: &[u8], mut edits: Vec<(Range<usize>, Vec<u8>)>) -> Vec<u8> {
    edits.sort_by_key(|(span, _)| span.start);
    let mut out = vec![];
    let mut cursor = 0;
    for (span, replacement) in edits {
        out.extend_from_slice(&bytes[cursor..span.start]);
        out.extend_from_slice(&replacement);
        cursor = span.end;
    }
    out.extend_from_slice(&bytes[cursor..]);
    out
}
