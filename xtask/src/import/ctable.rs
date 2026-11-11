//! Reader for the data tables in upstream C source files.
//!
//! This is not a C parser. It understands exactly the shapes the upstream
//! data files use (design D7):
//!
//! - `/* … */` and `// …` comments;
//! - preprocessor lines, of which only `#define NAME <number>` is remembered
//!   (it may give an array size);
//! - array initialisers `[qualifiers] type NAME[N] = { number, … };` with
//!   decimal or hexadecimal numbers, for 8-bit (`uint8_t`, `unsigned char`)
//!   and 32-bit (`uint32_t`) element types;
//! - pointer tables `[qualifiers] type * [const] NAME[N] = { element, … };`
//!   whose elements are identifiers, `NULL` or `0`; identifiers are resolved
//!   against the arrays of the same file.
//!
//! Every other top-level item (function definitions, declarations without an
//! initialiser) is skipped as a balanced run of tokens. Anything malformed
//! inside an initialiser, or anything unbalanced, is a
//! [`ImportError::Syntax`] carrying the file name and line. The reader never
//! panics, whatever bytes it is given.

use std::collections::BTreeMap;

use super::ImportError;

/// The contents of one array or pointer table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TableData {
    /// An array of 8-bit values.
    Bytes(Vec<u8>),
    /// An array of 32-bit values.
    Words(Vec<u32>),
    /// A pointer table: each element names an array of the same file, or is
    /// `None` for `NULL` / `0`.
    Pointers(Vec<Option<String>>),
}

/// One table read from a file, with the line its declaration starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Table {
    /// 1-based line of the table's name.
    pub(crate) line: usize,
    /// The elements.
    pub(crate) data: TableData,
}

/// All tables read from one source file, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableFile {
    file: String,
    tables: BTreeMap<String, Table>,
}

impl TableFile {
    /// The file name used in error messages.
    pub(crate) fn file(&self) -> &str {
        &self.file
    }

    /// Every table, in name order.
    #[cfg(test)]
    pub(crate) fn tables(&self) -> impl Iterator<Item = (&str, &Table)> {
        self.tables
            .iter()
            .map(|(name, table)| (name.as_str(), table))
    }

    /// The table called `name`, if the file defines one.
    pub(crate) fn get(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    fn require(&self, name: &str) -> Result<&Table, ImportError> {
        self.get(name).ok_or_else(|| ImportError::MissingTable {
            file: self.file.clone(),
            table: name.to_owned(),
        })
    }

    fn wrong_kind(&self, name: &str, table: &Table, expected: &str) -> ImportError {
        ImportError::Syntax {
            file: self.file.clone(),
            line: table.line,
            message: format!("table `{name}` is not {expected}"),
        }
    }

    /// The 8-bit array called `name`.
    pub(crate) fn bytes(&self, name: &str) -> Result<&[u8], ImportError> {
        let table = self.require(name)?;
        match &table.data {
            TableData::Bytes(values) => Ok(values),
            _ => Err(self.wrong_kind(name, table, "an 8-bit array")),
        }
    }

    /// The 32-bit array called `name`.
    pub(crate) fn words(&self, name: &str) -> Result<&[u32], ImportError> {
        let table = self.require(name)?;
        match &table.data {
            TableData::Words(values) => Ok(values),
            _ => Err(self.wrong_kind(name, table, "a 32-bit array")),
        }
    }

    /// The pointer table called `name`; every named element is guaranteed to
    /// be an array of this file.
    pub(crate) fn pointers(&self, name: &str) -> Result<&[Option<String>], ImportError> {
        let table = self.require(name)?;
        match &table.data {
            TableData::Pointers(values) => Ok(values),
            _ => Err(self.wrong_kind(name, table, "a pointer table")),
        }
    }
}

/// Reads every table of `text`; `file` names the source in error messages.
pub(crate) fn parse(file: &str, text: &[u8]) -> Result<TableFile, ImportError> {
    let lexed = Lexer::new(file, text).run()?;
    let mut parser = Parser {
        file,
        tokens: &lexed.tokens,
        pos: 0,
        defines: &lexed.defines,
        tables: BTreeMap::new(),
    };
    parser.run()?;
    let table_file = TableFile {
        file: file.to_owned(),
        tables: parser.tables,
    };
    resolve_pointers(&table_file)?;
    Ok(table_file)
}

/// Checks that every identifier in every pointer table names an array of the
/// same file.
fn resolve_pointers(table_file: &TableFile) -> Result<(), ImportError> {
    for (name, table) in &table_file.tables {
        let TableData::Pointers(elements) = &table.data else {
            continue;
        };
        for symbol in elements.iter().flatten() {
            let resolved = table_file
                .tables
                .get(symbol)
                .is_some_and(|target| !matches!(target.data, TableData::Pointers(_)));
            if !resolved {
                return Err(ImportError::UnknownSymbol {
                    file: table_file.file.clone(),
                    line: table.line,
                    table: name.clone(),
                    symbol: symbol.clone(),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Lexer

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Number(u64),
    /// A string or character literal (only ever skipped).
    Literal,
    Punct(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    line: usize,
}

#[derive(Debug)]
struct Lexed {
    tokens: Vec<Token>,
    defines: BTreeMap<String, u64>,
}

#[derive(Debug)]
struct Lexer<'a> {
    file: &'a str,
    text: &'a [u8],
    pos: usize,
    line: usize,
    /// True while only whitespace has been seen since the last newline.
    line_start: bool,
    tokens: Vec<Token>,
    defines: BTreeMap<String, u64>,
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

impl<'a> Lexer<'a> {
    fn new(file: &'a str, text: &'a [u8]) -> Self {
        Self {
            file,
            text,
            pos: 0,
            line: 1,
            line_start: true,
            tokens: Vec::new(),
            defines: BTreeMap::new(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.text.get(self.pos.saturating_add(offset)).copied()
    }

    /// Consumes one byte, counting lines.
    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        if byte == b'\n' {
            self.line += 1;
            self.line_start = true;
        }
        Some(byte)
    }

    fn syntax(&self, line: usize, message: impl Into<String>) -> ImportError {
        ImportError::Syntax {
            file: self.file.to_owned(),
            line,
            message: message.into(),
        }
    }

    fn run(mut self) -> Result<Lexed, ImportError> {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace() {
                self.bump();
            } else if byte == b'/' && self.peek_at(1) == Some(b'*') {
                self.block_comment()?;
            } else if byte == b'/' && self.peek_at(1) == Some(b'/') {
                self.line_comment();
            } else if byte == b'#' && self.line_start {
                self.directive()?;
            } else {
                self.line_start = false;
                let line = self.line;
                let kind = self.token()?;
                self.tokens.push(Token { kind, line });
            }
        }
        Ok(Lexed {
            tokens: self.tokens,
            defines: self.defines,
        })
    }

    fn block_comment(&mut self) -> Result<(), ImportError> {
        let start = self.line;
        self.bump();
        self.bump();
        loop {
            match self.bump() {
                None => return Err(self.syntax(start, "unterminated comment")),
                Some(b'*') if self.peek() == Some(b'/') => {
                    self.bump();
                    return Ok(());
                }
                Some(_) => {}
            }
        }
    }

    fn line_comment(&mut self) {
        while let Some(byte) = self.peek() {
            if byte == b'\n' {
                break;
            }
            self.bump();
        }
    }

    /// A preprocessor line. `#define NAME <number>` is remembered; every other
    /// directive, and every other `#define`, is ignored.
    fn directive(&mut self) -> Result<(), ImportError> {
        let line = self.line;
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if byte == b'\n' {
                break;
            }
            self.bump();
        }
        let text = self.text.get(start..self.pos).unwrap_or_default();
        let text = String::from_utf8_lossy(text);
        let mut words = text.trim_start_matches('#').split_whitespace();
        if words.next() == Some("define")
            && let (Some(name), Some(value), None) = (words.next(), words.next(), words.next())
            && name.bytes().next().is_some_and(is_ident_start)
            && name.bytes().all(is_ident_continue)
            && value.bytes().next().is_some_and(|b| b.is_ascii_digit())
        {
            let value = self.number_value(line, value.as_bytes())?;
            self.defines.insert(name.to_owned(), value);
        }
        Ok(())
    }

    fn token(&mut self) -> Result<TokenKind, ImportError> {
        let line = self.line;
        let start = self.pos;
        let Some(byte) = self.bump() else {
            return Err(self.syntax(line, "unexpected end of file"));
        };
        if is_ident_start(byte) {
            while self.peek().is_some_and(is_ident_continue) {
                self.bump();
            }
            let ident = self.text.get(start..self.pos).unwrap_or_default();
            return Ok(TokenKind::Ident(
                String::from_utf8_lossy(ident).into_owned(),
            ));
        }
        if byte.is_ascii_digit() {
            while self.peek().is_some_and(is_ident_continue) {
                self.bump();
            }
            let digits = self.text.get(start..self.pos).unwrap_or_default();
            return self.number_value(line, digits).map(TokenKind::Number);
        }
        if byte == b'"' || byte == b'\'' {
            self.literal(line, byte)?;
            return Ok(TokenKind::Literal);
        }
        Ok(TokenKind::Punct(byte))
    }

    /// Skips the rest of a string or character literal.
    fn literal(&mut self, line: usize, quote: u8) -> Result<(), ImportError> {
        loop {
            match self.peek() {
                None | Some(b'\n') => return Err(self.syntax(line, "unterminated literal")),
                Some(b'\\') => {
                    self.bump();
                    if self.peek() != Some(b'\n') {
                        self.bump();
                    }
                }
                Some(byte) => {
                    self.bump();
                    if byte == quote {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// The value of a decimal or hexadecimal literal. Octal (a leading zero
    /// followed by more digits) and suffixes are rejected.
    fn number_value(&self, line: usize, digits: &[u8]) -> Result<u64, ImportError> {
        let text = String::from_utf8_lossy(digits);
        let parsed = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
        {
            if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                None
            } else {
                u64::from_str_radix(hex, 16).ok()
            }
        } else if text.len() > 1 && text.starts_with('0') {
            None
        } else if text.bytes().all(|b| b.is_ascii_digit()) {
            text.parse::<u64>().ok()
        } else {
            None
        };
        parsed.ok_or_else(|| {
            self.syntax(
                line,
                format!("`{text}` is not a decimal or hexadecimal number"),
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Parser

/// The element type of a table, from its declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementKind {
    Byte,
    Word,
    Pointer,
}

#[derive(Debug)]
struct Parser<'a> {
    file: &'a str,
    tokens: &'a [Token],
    pos: usize,
    defines: &'a BTreeMap<String, u64>,
    tables: BTreeMap<String, Table>,
}

impl<'a> Parser<'a> {
    fn syntax(&self, line: usize, message: impl Into<String>) -> ImportError {
        ImportError::Syntax {
            file: self.file.to_owned(),
            line,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(token)
    }

    /// The line of the last token, for errors at the end of the file.
    fn last_line(&self) -> usize {
        self.tokens.last().map_or(1, |token| token.line)
    }

    fn run(&mut self) -> Result<(), ImportError> {
        while self.peek().is_some() {
            self.item()?;
        }
        Ok(())
    }

    /// One top-level item: a table definition, or anything else up to its
    /// terminating `;` or closing brace.
    fn item(&mut self) -> Result<(), ImportError> {
        let start = self.pos;
        let start_line = self.peek().map_or(1, |token| token.line);
        let mut depth: usize = 0;
        while let Some(token) = self.next() {
            let line = token.line;
            match token.kind {
                TokenKind::Punct(b'(' | b'[') => depth += 1,
                TokenKind::Punct(b')' | b']') => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| self.syntax(line, "unbalanced closing bracket"))?;
                }
                TokenKind::Punct(b';') if depth == 0 => return Ok(()),
                TokenKind::Punct(b'}') => return Err(self.syntax(line, "unbalanced `}`")),
                TokenKind::Punct(b'=') if depth == 0 => {
                    let head = self.tokens.get(start..self.pos - 1).unwrap_or_default();
                    return self.initialised_declaration(head, line);
                }
                TokenKind::Punct(b'{') if depth == 0 => {
                    self.skip_block(line)?;
                    if matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punct(b';'),
                            ..
                        })
                    ) {
                        self.pos += 1;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(self.syntax(start_line, "declaration is not terminated"))
    }

    /// Skips a brace-delimited block whose `{` has just been consumed.
    fn skip_block(&mut self, open_line: usize) -> Result<(), ImportError> {
        let mut depth: usize = 1;
        while let Some(token) = self.next() {
            match token.kind {
                TokenKind::Punct(b'{') => depth += 1,
                TokenKind::Punct(b'}') => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        Err(self.syntax(open_line, "unterminated block"))
    }

    /// A declaration whose `=` has just been consumed. `head` holds the
    /// tokens before the `=`.
    fn initialised_declaration(
        &mut self,
        head: &[Token],
        eq_line: usize,
    ) -> Result<(), ImportError> {
        let is_array = head
            .iter()
            .any(|token| token.kind == TokenKind::Punct(b'['));
        if !is_array {
            // A scalar initialiser: not a table, skip to its `;`.
            return self.skip_to_semicolon(eq_line);
        }
        let (name, line, kind, size) = self.array_head(head, eq_line)?;
        let open = self.next();
        match open {
            Some(Token {
                kind: TokenKind::Punct(b'{'),
                ..
            }) => {}
            Some(token) => {
                return Err(self.syntax(token.line, format!("expected `{{` to start `{name}`")));
            }
            None => return Err(self.syntax(line, format!("unterminated initialiser of `{name}`"))),
        }
        let data = self.elements(&name, line, kind)?;
        match self.next() {
            Some(Token {
                kind: TokenKind::Punct(b';'),
                ..
            }) => {}
            Some(token) => {
                let line = token.line;
                return Err(self.syntax(line, format!("expected `;` after `{name}`")));
            }
            None => {
                return Err(self.syntax(self.last_line(), format!("expected `;` after `{name}`")));
            }
        }
        let count = match &data {
            TableData::Bytes(values) => values.len(),
            TableData::Words(values) => values.len(),
            TableData::Pointers(values) => values.len(),
        };
        if let Some(size) = size
            && usize::try_from(size).ok() != Some(count)
        {
            return Err(self.syntax(
                line,
                format!("`{name}` declares {size} elements but initialises {count}"),
            ));
        }
        if self.tables.contains_key(&name) {
            return Err(self.syntax(line, format!("`{name}` is defined twice")));
        }
        self.tables.insert(name, Table { line, data });
        Ok(())
    }

    fn skip_to_semicolon(&mut self, line: usize) -> Result<(), ImportError> {
        let mut depth: usize = 0;
        while let Some(token) = self.next() {
            match token.kind {
                TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
                TokenKind::Punct(b')' | b']' | b'}') => {
                    let line = token.line;
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| self.syntax(line, "unbalanced closing bracket"))?;
                }
                TokenKind::Punct(b';') if depth == 0 => return Ok(()),
                _ => {}
            }
        }
        Err(self.syntax(line, "initialiser is not terminated"))
    }

    /// Interprets `qualifiers type [*] [const] NAME [ size? ]`.
    fn array_head(
        &self,
        head: &[Token],
        eq_line: usize,
    ) -> Result<(String, usize, ElementKind, Option<u64>), ImportError> {
        let bracket = head
            .iter()
            .position(|token| token.kind == TokenKind::Punct(b'['))
            .unwrap_or(head.len());
        let (before, after) = head.split_at(bracket);
        let Some((name_token, type_tokens)) = before.split_last() else {
            return Err(self.syntax(eq_line, "array declaration has no name"));
        };
        let TokenKind::Ident(name) = &name_token.kind else {
            return Err(self.syntax(name_token.line, "array declaration has no name"));
        };
        let line = name_token.line;

        let mut pointer = false;
        let mut element = None;
        for token in type_tokens {
            match &token.kind {
                TokenKind::Punct(b'*') if !pointer => pointer = true,
                TokenKind::Ident(word) => match word.as_str() {
                    "static" | "const" | "unsigned" => {}
                    "uint8_t" | "char" if element.is_none() && !pointer => {
                        element = Some(ElementKind::Byte);
                    }
                    "uint32_t" if element.is_none() && !pointer => {
                        element = Some(ElementKind::Word);
                    }
                    other => {
                        return Err(self.syntax(
                            token.line,
                            format!("unsupported element type `{other}` for `{name}`"),
                        ));
                    }
                },
                _ => {
                    return Err(self.syntax(
                        token.line,
                        format!("unexpected token in the declaration of `{name}`"),
                    ));
                }
            }
        }
        let kind = match (pointer, element) {
            (true, Some(_)) => ElementKind::Pointer,
            (false, Some(kind)) => kind,
            (_, None) => {
                return Err(self.syntax(line, format!("`{name}` has no supported element type")));
            }
        };

        let size = match after {
            [open, close] if is_punct(open, b'[') && is_punct(close, b']') => None,
            [open, size, close] if is_punct(open, b'[') && is_punct(close, b']') => {
                Some(self.size_value(name, size)?)
            }
            _ => {
                return Err(self.syntax(
                    line,
                    format!("`{name}` must be a one-dimensional array `{name}[N]`"),
                ));
            }
        };
        Ok((name.clone(), line, kind, size))
    }

    fn size_value(&self, name: &str, token: &Token) -> Result<u64, ImportError> {
        match &token.kind {
            TokenKind::Number(value) => Ok(*value),
            TokenKind::Ident(symbol) => {
                self.defines
                    .get(symbol)
                    .copied()
                    .ok_or_else(|| ImportError::UnknownSymbol {
                        file: self.file.to_owned(),
                        line: token.line,
                        table: name.to_owned(),
                        symbol: symbol.clone(),
                    })
            }
            _ => Err(self.syntax(token.line, format!("bad array size for `{name}`"))),
        }
    }

    /// The elements of an initialiser whose `{` has just been consumed, up to
    /// and including the closing `}`.
    fn elements(
        &mut self,
        name: &str,
        line: usize,
        kind: ElementKind,
    ) -> Result<TableData, ImportError> {
        let mut bytes = Vec::new();
        let mut words = Vec::new();
        let mut pointers = Vec::new();
        loop {
            // An element, or the closing brace (also after a trailing comma).
            let Some(token) = self.next() else {
                return Err(self.syntax(line, format!("unterminated initialiser of `{name}`")));
            };
            if is_punct(token, b'}') {
                break;
            }
            match (kind, &token.kind) {
                (ElementKind::Byte, TokenKind::Number(value)) => {
                    let value = u8::try_from(*value).map_err(|_| {
                        self.syntax(
                            token.line,
                            format!("{value} does not fit in a byte in `{name}`"),
                        )
                    })?;
                    bytes.push(value);
                }
                (ElementKind::Word, TokenKind::Number(value)) => {
                    let value = u32::try_from(*value).map_err(|_| {
                        self.syntax(
                            token.line,
                            format!("{value} does not fit in 32 bits in `{name}`"),
                        )
                    })?;
                    words.push(value);
                }
                (ElementKind::Pointer, TokenKind::Number(0)) => pointers.push(None),
                (ElementKind::Pointer, TokenKind::Ident(symbol)) if symbol == "NULL" => {
                    pointers.push(None);
                }
                (ElementKind::Pointer, TokenKind::Ident(symbol)) => {
                    pointers.push(Some(symbol.clone()));
                }
                _ => {
                    return Err(self.syntax(
                        token.line,
                        format!(
                            "unexpected {} in the initialiser of `{name}`",
                            describe(token)
                        ),
                    ));
                }
            }
            match self.next() {
                Some(next) if is_punct(next, b',') => {}
                Some(next) if is_punct(next, b'}') => break,
                Some(next) => {
                    return Err(self.syntax(
                        next.line,
                        format!(
                            "expected `,` or `}}` but found {} in `{name}`",
                            describe(next)
                        ),
                    ));
                }
                None => {
                    return Err(self.syntax(line, format!("unterminated initialiser of `{name}`")));
                }
            }
        }
        Ok(match kind {
            ElementKind::Byte => TableData::Bytes(bytes),
            ElementKind::Word => TableData::Words(words),
            ElementKind::Pointer => TableData::Pointers(pointers),
        })
    }
}

fn is_punct(token: &Token, byte: u8) -> bool {
    token.kind == TokenKind::Punct(byte)
}

fn describe(token: &Token) -> String {
    match &token.kind {
        TokenKind::Ident(name) => format!("identifier `{name}`"),
        TokenKind::Number(value) => format!("number {value}"),
        TokenKind::Literal => "literal".to_owned(),
        TokenKind::Punct(byte) if byte.is_ascii_graphic() => format!("`{}`", char::from(*byte)),
        TokenKind::Punct(byte) => format!("byte 0x{byte:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{TableData, TableFile, parse};
    use crate::import::ImportError;

    fn parse_text(text: &str) -> Result<TableFile, ImportError> {
        parse("fixture.c", text.as_bytes())
    }

    /// The line of the syntax error `text` produces, if it produces one.
    fn syntax_line(text: &str) -> Option<usize> {
        match parse_text(text) {
            Err(ImportError::Syntax { file, line, .. }) if file == "fixture.c" => Some(line),
            _ => None,
        }
    }

    #[test]
    fn ctable_reads_byte_array() -> Result<(), ImportError> {
        let file = parse_text(
            "#include \"def.h\"\n\
             static const uint8_t vgatest[]={\n\
             \x20 0x00,0xff,0x0A,\n\
             \x20 12,255,0};\n\
             const unsigned char plain[4] = { 1, 2, 3, 4, };\n",
        )?;
        assert_eq!(file.bytes("vgatest")?, [0x00, 0xff, 0x0a, 12, 255, 0]);
        assert_eq!(file.get("vgatest").map(|t| t.line), Some(2));
        assert_eq!(file.bytes("plain")?, [1, 2, 3, 4]);
        assert_eq!(file.tables().count(), 2);
        assert_eq!(file.file(), "fixture.c");
        // A value that does not fit in a byte is an error.
        assert_eq!(
            syntax_line("static const uint8_t b[] = {\n 256 };"),
            Some(2)
        );
        Ok(())
    }

    #[test]
    fn ctable_reads_u32_array() -> Result<(), ImportError> {
        let file = parse_text(
            "#include <SDL.h>\n\nuint32_t Icon[] = {\n    0x00000000, 0xFF102030, \n    \
             4294967295, 7\n}; \n",
        )?;
        assert_eq!(file.words("Icon")?, [0, 0xff10_2030, u32::MAX, 7]);
        assert_eq!(file.get("Icon").map(|t| t.line), Some(3));
        // A value that does not fit in 32 bits is an error.
        assert_eq!(syntax_line("uint32_t big[] = {\n0x100000000 };"), Some(2));
        Ok(())
    }

    #[test]
    fn ctable_skips_comments() -> Result<(), ImportError> {
        let file = parse_text(
            "/* Digger Remastered\n   Copyright */\n\
             // a line comment with { braces } and table[] = {1};\n\
             #define CTITLELEN 3\n\
             /* WARNING! */\n\
             static const uint8_t title[CTITLELEN] = { /* inline */ 120, // after\n\
             218, /* multi\nline */ 237\n};\n\
             /* End of WARNING */\n\
             void gettitle(unsigned char *buf)\n{\n\tuLongf n=CTITLELEN;\n\tint r;\n\
             \tr = uncompress(buf, &n, title, CTITLELEN);\n\tassert(r == 0); (void)r;\n}\n\
             extern const uint8_t * const ascii2vga[];\n",
        )?;
        assert_eq!(file.bytes("title")?, [120, 218, 237]);
        assert_eq!(file.get("title").map(|t| t.line), Some(6));
        assert_eq!(file.tables().count(), 1);
        Ok(())
    }

    #[test]
    fn ctable_reads_pointer_table() -> Result<(), ImportError> {
        let file = parse_text(
            "static const uint8_t a[]={1,2};\nstatic const uint8_t b[]={3};\n\
             const uint8_t *table[]={\n  a, b,   /* 0 */\n  b, a};\n\
             const uint8_t * const map[0x5]={\n  b,0,NULL,a,0};\n",
        )?;
        let some = |name: &str| Some(name.to_owned());
        assert_eq!(
            file.pointers("table")?,
            [some("a"), some("b"), some("b"), some("a")]
        );
        assert_eq!(
            file.pointers("map")?,
            [some("b"), None, None, some("a"), None]
        );
        // Every named element resolves to an array of the same file.
        for entry in file.pointers("table")?.iter().flatten() {
            assert!(matches!(
                file.get(entry).map(|t| &t.data),
                Some(TableData::Bytes(_))
            ));
        }
        // Accessing a table as the wrong kind or by a wrong name is an error.
        assert!(matches!(
            file.bytes("table"),
            Err(ImportError::Syntax { line: 3, .. })
        ));
        assert!(matches!(
            file.pointers("missing"),
            Err(ImportError::MissingTable { ref table, .. }) if table == "missing"
        ));
        Ok(())
    }

    #[test]
    fn ctable_unknown_symbol_is_error() {
        let error = parse_text(
            "static const uint8_t a[]={1};\n\nconst uint8_t *table[]={\n  a, vgamissing};\n",
        )
        .err();
        assert!(
            matches!(
                &error,
                Some(ImportError::UnknownSymbol { file, line: 3, table, symbol })
                    if file == "fixture.c" && table == "table" && symbol == "vgamissing"
            ),
            "{error:?}"
        );
        // A pointer table is not a valid target of another pointer table.
        let error =
            parse_text("uint8_t a[]={1};\nconst uint8_t *t[]={a};\nconst uint8_t *u[]={t};").err();
        assert!(
            matches!(&error, Some(ImportError::UnknownSymbol { symbol, .. }) if symbol == "t"),
            "{error:?}"
        );
        // An undefined size macro is an unknown symbol too.
        let error = parse_text("uint8_t a[LEN]={1};").err();
        assert!(
            matches!(&error, Some(ImportError::UnknownSymbol { symbol, .. }) if symbol == "LEN"),
            "{error:?}"
        );
    }

    #[test]
    fn ctable_reports_line_of_bad_token() {
        let text = "static const uint8_t a[]={\n  0x00,0x01,\n  0x02,oops,0x03,\n  0x04};\n";
        assert_eq!(syntax_line(text), Some(3));
        let message = parse_text(text)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(message.contains("fixture.c:3"), "{message}");
        assert!(message.contains("oops"), "{message}");
        // Other malformed numbers, separators and declarations.
        assert_eq!(syntax_line("uint8_t a[]={\n1,\n0xZZ};"), Some(3));
        assert_eq!(syntax_line("uint8_t a[]={\n\n012};"), Some(3));
        assert_eq!(syntax_line("uint8_t a[]={1\n2};"), Some(2));
        assert_eq!(syntax_line("uint8_t a[]={1,,2};"), Some(1));
        assert_eq!(syntax_line("uint8_t a[]={-1};"), Some(1));
        assert_eq!(syntax_line("const uint8_t *t[]={\n 5};"), Some(2));
        assert_eq!(syntax_line("\nfloat a[]={1};"), Some(2));
        assert_eq!(syntax_line("uint8_t a[2][2]={1};"), Some(1));
        assert_eq!(syntax_line("uint8_t a[3]={1,2};"), Some(1));
        assert_eq!(syntax_line("uint8_t a[]={1};\nuint8_t a[]={2};"), Some(2));
        assert_eq!(syntax_line("uint8_t a[]={1}\nuint8_t b[]={2};"), Some(2));
    }

    #[test]
    fn ctable_unterminated_initialiser_is_error() {
        let text = "/* header */\nstatic const uint8_t a[]={\n  0x00,0x01,\n  0x02,\n";
        assert_eq!(syntax_line(text), Some(2));
        let message = parse_text(text)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(message.contains("unterminated"), "{message}");
        // Unterminated at other places.
        assert_eq!(syntax_line("uint8_t a[]={1,2}"), Some(1));
        assert_eq!(syntax_line("uint8_t a[]="), Some(1));
        assert_eq!(syntax_line("\n/* never closed\n uint8_t a[]={1};"), Some(2));
        assert_eq!(syntax_line("void f(void)\n{\n  if (x) {\n"), Some(2));
        assert_eq!(syntax_line("uint8_t a[]={1};\n}"), Some(2));
        assert_eq!(syntax_line("char *s = \"open\n;"), Some(1));
    }

    /// A small deterministic generator (xorshift64*), so the test needs no
    /// extra dependency.
    #[derive(Debug)]
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        fn below(&mut self, bound: usize) -> usize {
            let bound = u64::try_from(bound).unwrap_or(u64::MAX).max(1);
            usize::try_from(self.next() % bound).unwrap_or(0)
        }
    }

    #[test]
    fn ctable_never_panics_on_random_input() {
        const PIECES: &[&str] = &[
            "static ",
            "const ",
            "uint8_t ",
            "uint32_t ",
            "unsigned char ",
            "*",
            "name",
            "NULL",
            "[",
            "]",
            "=",
            "{",
            "}",
            ";",
            ",",
            "0x",
            "0xff",
            "12",
            "0",
            "08",
            "99999999999999999999999",
            "/*",
            "*/",
            "//",
            "\n",
            "#define N 4\n",
            "#",
            "\"",
            "'",
            "\\",
            "(",
            ")",
            " ",
        ];
        let mut rng = Rng(0x0123_4567_89ab_cdef);
        for case in 0..200 {
            let len = rng.below(400);
            let mut text = Vec::with_capacity(len);
            for _ in 0..len {
                if case % 2 == 0 {
                    // Raw bytes of any value.
                    text.push(rng.next().to_le_bytes().first().copied().unwrap_or(0));
                } else {
                    let piece = PIECES.get(rng.below(PIECES.len())).copied().unwrap_or(" ");
                    text.extend_from_slice(piece.as_bytes());
                }
            }
            // Ok or Err are both fine; a panic is not.
            let outcome = std::panic::catch_unwind(|| parse("random.c", &text).map(|_| ()));
            assert!(outcome.is_ok(), "reader panicked on case {case}: {text:?}");
        }
    }
}
