use crate::token::{Keyword, Token, TokenWithSpan};

// ── Configuration Types ────────────────────────────────────────────────────────

/// Keyword casing mode for SQL formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeywordCase {
    /// Preserve original casing from source
    #[default]
    Preserve,
    /// Convert all keywords to UPPERCASE
    Upper,
    /// Convert all keywords to lowercase
    Lower,
}

/// Comma positioning style for column lists
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommaStyle {
    /// Comma at end of line: `col1, col2, col3`
    #[default]
    Trailing,
    /// Comma at start of line: `col1\n, col2\n, col3`
    Leading,
}

/// Configuration for SQL formatting
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormatConfig {
    /// Number of spaces per indentation level (default: 2)
    pub indent_width: usize,
    /// Keyword casing mode (default: Preserve)
    pub keyword_case: KeywordCase,
    /// Comma positioning in lists (default: Trailing)
    pub comma_style: CommaStyle,
    /// Maximum line width before wrapping (default: 120, 0 = no wrapping)
    pub line_width: usize,
    /// Convert keywords to uppercase (legacy compat, overrides keyword_case when true)
    pub uppercase_keywords: bool,
    /// Put semicolons on their own line (default: true)
    pub semicolon_newline: bool,
    /// Put each SELECT target expression on its own line (default: true)
    pub select_newline: bool,
    /// Put WHERE/AND/OR on new lines (default: true)
    pub logical_operator_newline: bool,
    /// Indentation width (spaces) for PL/SQL block bodies (BEGIN/END,
    /// PACKAGE BODY members). `None` means fall back to `indent_width`
    /// (default: None)
    pub block_indent: Option<usize>,
    /// Preserve blank lines from the input in the formatted output
    /// (default: false)
    pub preserve_blank_lines: bool,
    /// Insert a blank line between PROCEDURE/FUNCTION definitions inside
    /// a PACKAGE BODY (default: true)
    pub procedure_newline: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            keyword_case: KeywordCase::Preserve,
            comma_style: CommaStyle::Trailing,
            line_width: 120,
            uppercase_keywords: false,
            semicolon_newline: true,
            select_newline: true,
            logical_operator_newline: true,
            block_indent: None,
            preserve_blank_lines: false,
            procedure_newline: true,
        }
    }
}

// ── Indent tracking ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndentKind {
    Begin,
    If,
    Loop,
    Case,
    Select,
    CreateTableBody,
    Subquery,
    Insert,
    Update,
    Merge,
    Cte,
    UpdateSet,
    WhereCont,
    ParenExpr,
    Package,
}

// ── TokenFormatter ─────────────────────────────────────────────────────────────

pub struct TokenFormatter<'a> {
    source: &'a str,
    tokens: Vec<TokenWithSpan>,
    pos: usize,
    indent_stack: Vec<IndentKind>,
    needs_line: bool,
    output: String,
    config: FormatConfig,
    /// Parenthesis nesting depth (for subquery / nested expression detection)
    paren_depth: usize,
    /// Tracks the paren_depth at which BETWEEN was last seen.
    /// When AND is encountered at the same depth, it is treated as
    /// BETWEEN's partner (not a logical AND) and kept inline.
    between_depth: Option<i32>,
    /// Parallel to indent_stack: records the paren_depth at which each
    /// IndentKind::Subquery was pushed.  Used in RParen to only pop the
    /// Subquery indent when the matching `)` is encountered, avoiding
    /// false pops for function-call parens inside subqueries.
    subquery_depths: Vec<usize>,
    /// Parallel to indent_stack for IndentKind::ParenExpr: records the
    /// paren_depth at which each expression-paren was opened, so RParen
    /// can pop the correct indent level.
    paren_expr_depths: Vec<usize>,
    /// True when inside a DML statement (SELECT / INSERT / UPDATE / DELETE
    /// / MERGE / WITH).  AND/OR continuation indent is only applied in
    /// DML contexts to avoid false positives in DDL like CREATE OR REPLACE.
    dml_stmt_active: bool,
}

impl<'a> TokenFormatter<'a> {
    /// Create formatter with default configuration (backward compatible)
    pub fn new(source: &'a str, tokens: Vec<TokenWithSpan>) -> Self {
        Self::with_config(source, tokens, FormatConfig::default())
    }

    /// Create formatter with custom configuration
    pub fn with_config(source: &'a str, tokens: Vec<TokenWithSpan>, mut config: FormatConfig) -> Self {
        // Legacy compat: uppercase_keywords=true overrides keyword_case
        if config.uppercase_keywords {
            config.keyword_case = KeywordCase::Upper;
        }
        Self {
            source,
            tokens,
            pos: 0,
            indent_stack: Vec::new(),
            needs_line: false,
            output: String::new(),
            config,
            paren_depth: 0,
            between_depth: None,
            subquery_depths: Vec::new(),
            paren_expr_depths: Vec::new(),
            dml_stmt_active: false,
        }
    }

    pub fn format(mut self) -> String {
        while self.pos < self.tokens.len() {
            let tws = &self.tokens[self.pos];
            match &tws.token {
                Token::Eof => break,
                Token::Comment(ref s) => {
                    let comment = s.clone();
                    if self.needs_line {
                        self.flush_pending_line();
                    } else if !self.output.is_empty() {
                        self.output.push('\n');
                        self.emit_indent();
                    }
                    self.output.push_str(&comment);
                    self.needs_line = true;
                    self.pos += 1;
                }
                _ => {
                    self.handle_token();
                }
            }
        }
        self.output
    }

    // ── Indent / newline helpers ───────────────────────────────────────────────

    fn flush_pending_line(&mut self) {
        if self.config.preserve_blank_lines && self.had_blank_line_before() {
            self.output.push('\n');
        }
        self.output.push('\n');
        self.emit_indent();
        self.needs_line = false;
    }

    fn emit_line_start(&mut self) {
        if self.needs_line {
            self.flush_pending_line();
        } else if !self.output.is_empty() {
            if self.config.preserve_blank_lines && self.had_blank_line_before() {
                self.output.push('\n');
            }
            self.output.push('\n');
            self.emit_indent();
        }
    }

    fn emit_newline_if_needed(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') && !self.output.ends_with(' ') {
            self.output.push('\n');
        }
    }

    fn emit_indent(&mut self) {
        let spaces = self.indent_stack.len() * self.effective_indent_width();
        for _ in 0..spaces {
            self.output.push(' ');
        }
    }

    /// Indentation width to use for the current nesting level. When
    /// `block_indent` is configured and the formatter is currently inside a
    /// PL/SQL block or PACKAGE BODY, it overrides `indent_width`.
    fn effective_indent_width(&self) -> usize {
        if let Some(block_indent) = self.config.block_indent {
            if self.in_pl_or_package_block() {
                return block_indent;
            }
        }
        self.config.indent_width
    }

    fn in_pl_or_package_block(&self) -> bool {
        self.indent_stack.iter().any(|k| {
            matches!(k, IndentKind::Begin | IndentKind::If | IndentKind::Loop | IndentKind::Case | IndentKind::Package)
        })
    }

    /// True when the source contains a blank line between the previous
    /// token and the current one (used by `preserve_blank_lines`).
    fn had_blank_line_before(&self) -> bool {
        if self.pos == 0 || self.pos >= self.tokens.len() {
            return false;
        }
        let prev_end = self.tokens[self.pos - 1].span.end;
        let cur_start = self.tokens[self.pos].span.start;
        if cur_start <= prev_end {
            return false;
        }
        self.source[prev_end..cur_start].matches('\n').count() >= 2
    }

    fn emit_space(&mut self) {
        if !self.output.ends_with(' ') && !self.output.ends_with('\n') {
            self.output.push(' ');
        }
    }

    // ── Token emission ─────────────────────────────────────────────────────────

    /// Emit current token with keyword casing transformation applied
    fn emit_current_token(&mut self) {
        let tws = &self.tokens[self.pos];
        let text = &self.source[tws.span.start..tws.span.end];
        let transformed = self.transform_text_at(text, &tws.token, self.pos);
        self.output.push_str(&transformed);
    }

    /// Apply keyword casing transformation, unless `pos` follows a `Token::Dot`.
    ///
    /// Some keywords (e.g. `NAME`) are also common identifiers. When such a
    /// token appears right after a `.` (as in `a.name`), it is being used as
    /// an identifier/member-access, not as an actual SQL keyword, so its
    /// casing must be preserved instead of being forced to upper/lower case
    /// (issue #311).
    fn transform_text_at(&self, text: &str, token: &Token, pos: usize) -> String {
        let follows_dot = pos > 0 && matches!(&self.tokens[pos - 1].token, Token::Dot);
        if follows_dot {
            return text.to_string();
        }
        self.transform_text(text, token)
    }

    /// Apply keyword casing transformation
    fn transform_text(&self, text: &str, token: &Token) -> String {
        if matches!(token, Token::Keyword(_)) {
            match self.config.keyword_case {
                KeywordCase::Preserve => text.to_string(),
                KeywordCase::Upper => text.to_uppercase(),
                KeywordCase::Lower => text.to_lowercase(),
            }
        } else {
            text.to_string()
        }
    }

    fn current_line_length(&self) -> usize {
        self.output.rfind('\n').map(|pos| self.output.len() - pos - 1).unwrap_or(self.output.len())
    }

    fn would_exceed_line_width(&self, token_text: &str) -> bool {
        if self.config.line_width == 0 {
            return false;
        }
        self.current_line_length() + token_text.len() > self.config.line_width
    }

    fn emit_default_token(&mut self) {
        let tws = &self.tokens[self.pos];
        let is_space_rejecting =
            matches!(&tws.token, Token::Comma | Token::Semicolon | Token::RParen | Token::RBracket | Token::Dot);
        let prev_rejects_space = self.pos > 0 && {
            let prev = &self.tokens[self.pos - 1].token;
            matches!(prev, Token::LParen | Token::LBracket | Token::Comma | Token::Dot)
        };
        let text = self.transform_text_at(&self.source[tws.span.start..tws.span.end], &tws.token, self.pos);
        let pos = self.pos;
        let _ = tws;

        if self.needs_line {
            self.flush_pending_line();
        }

        if !is_space_rejecting
            && !prev_rejects_space
            && !self.output.is_empty()
            && !self.output.ends_with(' ')
            && !self.output.ends_with('\n')
        {
            self.output.push(' ');
        }

        // Soft line wrapping: if this token would exceed line width, and we're not
        // in a structured list (where newlines are already handled), insert a newline
        if self.config.line_width > 0 && !self.in_create_table_body() && self.would_exceed_line_width(&text) {
            self.output.push('\n');
            self.emit_indent();
        }

        self.output.push_str(&text);
        self.pos = pos + 1;
    }

    // ── Peek helpers ───────────────────────────────────────────────────────────

    fn peek_token(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset).map(|t| &t.token)
    }

    fn peek_token_back(&self, offset: usize) -> Option<&Token> {
        if self.pos >= offset {
            self.tokens.get(self.pos - offset).map(|t| &t.token)
        } else {
            None
        }
    }

    // ── Indent stack helpers ───────────────────────────────────────────────────

    fn pop_indent_to(&mut self, kind: IndentKind) {
        while let Some(top) = self.indent_stack.last() {
            if *top == kind {
                self.indent_stack.pop();
                break;
            } else {
                self.indent_stack.pop();
            }
        }
    }

    fn in_select_context(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::Select))
    }

    fn in_subquery(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::Subquery))
    }

    fn in_pl_block(&self) -> bool {
        self.indent_stack
            .iter()
            .any(|k| matches!(k, IndentKind::Begin | IndentKind::If | IndentKind::Loop | IndentKind::Case))
    }

    fn in_insert_context(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::Insert))
    }

    fn in_update_context(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::Update))
    }

    fn in_merge_context(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::Merge))
    }

    fn in_cte_context(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::Cte))
    }

    fn in_where_cont(&self) -> bool {
        self.indent_stack.iter().any(|k| matches!(k, IndentKind::WhereCont))
    }

    fn pop_where_cont(&mut self) {
        while self.indent_stack.last() == Some(&IndentKind::WhereCont) {
            self.indent_stack.pop();
        }
    }

    /// Detect `CREATE [OR REPLACE] PACKAGE BODY` starting at the current
    /// (`CREATE`) token position.
    fn is_create_package_body(&self) -> bool {
        let mut offset = 1;
        if matches!(self.peek_token(offset), Some(Token::Keyword(Keyword::OR))) {
            if !matches!(self.peek_token(offset + 1), Some(Token::Keyword(Keyword::REPLACE))) {
                return false;
            }
            offset += 2;
        }
        matches!(self.peek_token(offset), Some(Token::Keyword(Keyword::PACKAGE)))
            && matches!(self.peek_token(offset + 1), Some(Token::Keyword(Keyword::BODY_P)))
    }

    fn is_procedure_or_function_context(&self) -> bool {
        let mut i = self.pos;
        while i > 0 {
            i -= 1;
            if let Some(tws) = self.tokens.get(i) {
                match &tws.token {
                    Token::Keyword(Keyword::PROCEDURE) | Token::Keyword(Keyword::FUNCTION) => {
                        return true;
                    }
                    Token::Keyword(Keyword::BEGIN_P) | Token::Keyword(Keyword::DECLARE) => {
                        return false;
                    }
                    _ => {}
                }
            }
        }
        false
    }

    // ── Main token dispatch ────────────────────────────────────────────────────

    fn handle_token(&mut self) {
        let token = &self.tokens[self.pos].token;
        let next_token = self.peek_token(1);

        match token {
            // ── PL/pgSQL: BEGIN/END ────────────────────────────────────────────
            Token::Keyword(Keyword::BEGIN_P) => {
                self.emit_newline_if_needed();
                self.emit_indent();
                self.emit_current_token();
                self.indent_stack.push(IndentKind::Begin);
                self.pos += 1;
                self.needs_line = true;
            }

            Token::Keyword(Keyword::THEN) => {
                self.emit_space();
                self.emit_current_token();
                self.indent_stack.push(IndentKind::If);
                self.pos += 1;
                self.needs_line = true;
            }

            Token::Keyword(Keyword::LOOP) => {
                let prev_token = self.peek_token_back(1);
                if !matches!(prev_token, Some(Token::Keyword(Keyword::END_P))) {
                    self.emit_newline_if_needed();
                    self.emit_indent();
                    self.emit_current_token();
                    self.indent_stack.push(IndentKind::Loop);
                    self.pos += 1;
                    self.needs_line = true;
                } else {
                    self.emit_default_token();
                }
            }

            Token::Keyword(Keyword::END_P) => match next_token {
                Some(Token::Keyword(Keyword::IF_P)) => {
                    self.pop_indent_to(IndentKind::If);
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    self.emit_space();
                    self.emit_current_token();
                    self.pos += 1;
                }
                Some(Token::Keyword(Keyword::LOOP)) => {
                    self.pop_indent_to(IndentKind::Loop);
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    self.emit_space();
                    self.emit_current_token();
                    self.pos += 1;
                }
                Some(Token::Keyword(Keyword::CASE)) => {
                    self.pop_indent_to(IndentKind::Case);
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    self.emit_space();
                    self.emit_current_token();
                    self.pos += 1;
                }
                Some(Token::Ident(_)) if self.indent_stack.last() == Some(&IndentKind::Package) => {
                    self.pop_indent_to(IndentKind::Package);
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    self.emit_space();
                    self.emit_current_token();
                    self.pos += 1;
                }
                _ => {
                    self.pop_indent_to(IndentKind::Begin);
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                }
            },

            Token::Ident(name) if name.to_uppercase() == "EXCEPTION" => {
                self.pop_indent_to(IndentKind::Begin);
                self.emit_line_start();
                self.emit_current_token();
                self.indent_stack.push(IndentKind::Begin);
                self.needs_line = true;
                self.pos += 1;
            }

            Token::Keyword(Keyword::WHEN) => {
                if self.in_merge_context() {
                    self.pop_indent_to(IndentKind::Merge);
                    self.indent_stack.pop();
                    self.indent_stack.push(IndentKind::Merge);
                }
                self.emit_line_start();
                self.emit_current_token();
                if self.in_merge_context() {
                    self.emit_space();
                }
                self.pos += 1;
            }

            Token::Keyword(Keyword::ELSE) => {
                self.emit_line_start();
                self.emit_current_token();
                self.needs_line = true;
                self.pos += 1;
            }

            Token::Ident(name) if name.to_uppercase() == "ELSIF" => {
                self.pop_indent_to(IndentKind::If);
                self.emit_line_start();
                self.emit_current_token();
                self.needs_line = true;
                self.pos += 1;
            }

            // ── Semicolon ──────────────────────────────────────────────────────
            Token::Semicolon => {
                self.emit_current_token();
                self.dml_stmt_active = false;
                // Pop DML/CTE indent contexts, but keep PL/pgSQL block contexts
                self.indent_stack.retain(|k| {
                    matches!(
                        k,
                        IndentKind::Begin | IndentKind::If | IndentKind::Loop | IndentKind::Case | IndentKind::Package
                    )
                });
                if self.config.semicolon_newline {
                    self.needs_line = true;
                } else {
                    self.emit_space();
                }
                self.pos += 1;
            }

            // ── SELECT ─────────────────────────────────────────────────────────
            Token::Keyword(Keyword::SELECT) => {
                self.dml_stmt_active = true;
                self.emit_line_start();
                self.emit_current_token();
                self.indent_stack.push(IndentKind::Select);
                self.pos += 1;
                if self.config.select_newline {
                    self.needs_line = true;
                } else {
                    self.emit_space();
                }
            }

            // ── FROM / WHERE / GROUP BY / HAVING / ORDER / LIMIT / OFFSET / UNION ─
            Token::Keyword(Keyword::FROM) => {
                if self.in_select_context() {
                    self.pop_indent_to(IndentKind::Select);
                    self.emit_line_start();
                } else {
                    self.emit_space();
                }
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            Token::Keyword(Keyword::WHERE)
            | Token::Keyword(Keyword::GROUP_P)
            | Token::Keyword(Keyword::HAVING)
            | Token::Keyword(Keyword::ORDER)
            | Token::Keyword(Keyword::LIMIT)
            | Token::Keyword(Keyword::OFFSET)
            | Token::Keyword(Keyword::UNION)
            | Token::Keyword(Keyword::INTERSECT)
            | Token::Keyword(Keyword::EXCEPT) => {
                if self.paren_depth == 0 {
                    if !self.in_subquery() {
                        self.pop_where_cont();
                    }
                    if self.in_select_context() {
                        self.pop_indent_to(IndentKind::Select);
                    }
                    if self.in_update_context() {
                        self.pop_indent_to(IndentKind::Update);
                    }
                }
                self.emit_line_start();
                self.emit_current_token();
                self.pos += 1;
            }

            // ── JOIN types ─────────────────────────────────────────────────────
            Token::Keyword(Keyword::INNER_P)
            | Token::Keyword(Keyword::LEFT)
            | Token::Keyword(Keyword::RIGHT)
            | Token::Keyword(Keyword::FULL)
            | Token::Keyword(Keyword::CROSS)
            | Token::Keyword(Keyword::JOIN) => {
                if self.paren_depth == 0 {
                    self.emit_line_start();
                } else {
                    self.emit_space();
                }
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            // ── AND / OR ──────────────────────────────────────────────────────
            Token::Keyword(Keyword::AND) => {
                // BETWEEN ... AND: keep the AND inline (it is BETWEEN's partner)
                if let Some(bd) = self.between_depth {
                    if bd == self.paren_depth as i32 {
                        self.emit_default_token();
                        self.between_depth = None;
                        // emit_default_token already advanced self.pos
                        return;
                    }
                    self.between_depth = None;
                }
                if self.config.logical_operator_newline {
                    if self.dml_stmt_active && !self.in_subquery() && !self.in_where_cont() {
                        self.indent_stack.push(IndentKind::WhereCont);
                    }
                    self.emit_line_start();
                    self.emit_current_token();
                    self.emit_space();
                    self.pos += 1;
                } else {
                    self.emit_default_token();
                }
            }
            Token::Keyword(Keyword::OR) => {
                if self.config.logical_operator_newline {
                    if self.dml_stmt_active && !self.in_subquery() && !self.in_where_cont() {
                        self.indent_stack.push(IndentKind::WhereCont);
                    }
                    self.emit_line_start();
                    self.emit_current_token();
                    self.emit_space();
                    self.pos += 1;
                } else {
                    self.emit_default_token();
                }
            }

            // ── Comma ──────────────────────────────────────────────────────────
            Token::Comma => {
                let in_list = (matches!(self.indent_stack.last(), Some(IndentKind::Select))
                    && self.config.select_newline)
                    || self.in_create_table_body()
                    || self.in_insert_context()
                    || self.in_update_context();
                if in_list {
                    self.handle_list_comma();
                } else {
                    self.emit_current_token();
                    self.emit_space();
                }
                self.pos += 1;
            }

            // ── Parentheses ───────────────────────────────────────────────────
            Token::LParen => {
                self.emit_current_token();
                self.paren_depth += 1;
                self.pos += 1;
                if let Some(Token::Keyword(Keyword::SELECT)) = self.peek_token(0) {
                    self.subquery_depths.push(self.paren_depth);
                    self.indent_stack.push(IndentKind::Subquery);
                    self.needs_line = true;
                } else {
                    self.paren_expr_depths.push(self.paren_depth);
                    self.indent_stack.push(IndentKind::ParenExpr);
                }
            }

            Token::RParen => {
                // If this RParen matches the LParen that opened CreateTableBody, exit
                if self.in_create_table_body() && self.paren_depth == 1 {
                    self.indent_stack.pop();
                    self.paren_depth = 0;
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                } else {
                    // Only pop Subquery indent when this ) matches the depth
                    // of the ( that pushed it (avoid false pops for
                    // function-call parens inside subqueries).
                    if let Some(&depth) = self.subquery_depths.last() {
                        if self.paren_depth == depth {
                            self.subquery_depths.pop();
                            self.indent_stack.pop();
                            self.emit_line_start();
                        }
                    }
                    if let Some(&depth) = self.paren_expr_depths.last() {
                        if self.paren_depth == depth {
                            self.paren_expr_depths.pop();
                            self.indent_stack.pop();
                        }
                    }
                    if self.paren_depth > 0 {
                        self.paren_depth -= 1;
                    }
                    self.emit_current_token();
                    self.pos += 1;
                }
            }

            // ── PROCEDURE / FUNCTION ──────────────────────────────────────────
            Token::Keyword(Keyword::PROCEDURE) | Token::Keyword(Keyword::FUNCTION) => {
                let follows_package_member = self.indent_stack.last() == Some(&IndentKind::Package)
                    && matches!(self.peek_token_back(1), Some(Token::Semicolon));
                if follows_package_member && self.config.procedure_newline {
                    if self.needs_line {
                        self.output.push('\n');
                        self.needs_line = false;
                    }
                    self.output.push('\n');
                    self.emit_indent();
                } else {
                    self.emit_line_start();
                }
                self.emit_current_token();
                self.pos += 1;
            }

            Token::Keyword(Keyword::IS) | Token::Keyword(Keyword::AS) => {
                self.emit_space();
                self.emit_current_token();
                self.pos += 1;
                if self.is_procedure_or_function_context() {
                    self.needs_line = true;
                }
            }

            // ── IF ────────────────────────────────────────────────────────────
            Token::Keyword(Keyword::IF_P) => {
                self.emit_line_start();
                self.emit_current_token();
                self.pos += 1;
            }

            // ── WHILE / FOR (PL/pgSQL loops) ──────────────────────────────────
            Token::Keyword(Keyword::WHILE_P) => {
                self.emit_line_start();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            Token::Keyword(Keyword::FOR) => {
                if self.in_pl_block() {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.emit_space();
                } else {
                    self.emit_default_token();
                }
                self.pos += 1;
            }

            // ── CASE ──────────────────────────────────────────────────────────
            Token::Keyword(Keyword::CASE) => {
                self.emit_line_start();
                self.emit_current_token();
                self.indent_stack.push(IndentKind::Case);
                self.pos += 1;
                self.needs_line = true;
            }

            // ── RETURN / EXECUTE (PL/pgSQL statements) ───────────────────────
            Token::Keyword(Keyword::RETURN) | Token::Keyword(Keyword::EXECUTE) => {
                if self.in_pl_block() {
                    self.emit_line_start();
                } else {
                    self.emit_space();
                }
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            // ── RAISE / PERFORM (identifiers in PL/pgSQL, not keywords) ─────────
            Token::Ident(name) if matches!(name.to_uppercase().as_str(), "RAISE" | "PERFORM") => {
                if self.in_pl_block() {
                    self.emit_line_start();
                }
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            // ── CREATE TABLE / CREATE PACKAGE BODY ────────────────────────────
            Token::Keyword(Keyword::CREATE) => {
                // Check if followed by TABLE
                if let Some(Token::Keyword(Keyword::TABLE)) = next_token {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    // Set flag to detect opening paren for column list
                    self.handle_create_table();
                } else if self.is_create_package_body() {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    self.indent_stack.push(IndentKind::Package);
                } else {
                    self.emit_default_token();
                }
            }

            // ── INSERT INTO ... VALUES ──────────────────────────────────────
            Token::Keyword(Keyword::INSERT) => {
                self.dml_stmt_active = true;
                self.emit_line_start();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
                if !self.in_merge_context() {
                    self.indent_stack.push(IndentKind::Insert);
                }
            }

            // ── DELETE FROM ─────────────────────────────────────────────────
            Token::Keyword(Keyword::DELETE_P) => {
                self.dml_stmt_active = true;
                self.emit_line_start();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            // ── UPDATE ... SET ──────────────────────────────────────────────
            Token::Keyword(Keyword::UPDATE) => {
                self.dml_stmt_active = true;
                self.emit_line_start();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
                if !self.in_merge_context() {
                    self.indent_stack.push(IndentKind::Update);
                }
            }

            // ── MERGE INTO ... USING ... ON ... WHEN ────────────────────────
            Token::Keyword(Keyword::MERGE) => {
                self.dml_stmt_active = true;
                self.emit_line_start();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
                self.indent_stack.push(IndentKind::Merge);
            }

            // ── WITH (CTE) ─────────────────────────────────────────────────
            Token::Keyword(Keyword::WITH) => {
                self.dml_stmt_active = true;
                self.emit_line_start();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
                self.indent_stack.push(IndentKind::Cte);
            }

            // ── SET (in UPDATE context) ─────────────────────────────────────
            Token::Keyword(Keyword::SET) => {
                if self.in_update_context() && !self.in_merge_context() {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.indent_stack.push(IndentKind::UpdateSet);
                    self.pos += 1;
                    self.needs_line = true;
                } else if self.in_merge_context() {
                    self.emit_space();
                    self.emit_current_token();
                    self.emit_space();
                    self.pos += 1;
                } else {
                    self.emit_default_token();
                }
            }

            // ── VALUES (in INSERT context) ──────────────────────────────────
            Token::Keyword(Keyword::VALUES) => {
                if self.in_insert_context() {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                    self.needs_line = true;
                } else {
                    self.emit_default_token();
                }
            }

            // ── USING (in MERGE context) ────────────────────────────────────
            Token::Keyword(Keyword::USING) => {
                if self.in_merge_context() {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                } else {
                    self.emit_default_token();
                }
            }

            // ── ON (in MERGE context) ───────────────────────────────────────
            Token::Keyword(Keyword::ON) => {
                if self.in_merge_context() {
                    self.emit_line_start();
                    self.emit_current_token();
                    self.pos += 1;
                } else {
                    self.emit_default_token();
                }
            }

            // ── INTO ────────────────────────────────────────────────────────
            Token::Keyword(Keyword::INTO) => {
                self.emit_space();
                self.emit_current_token();
                self.emit_space();
                self.pos += 1;
            }

            // ── BETWEEN (needed for BETWEEN ... AND tracking) ─────────────────
            Token::Keyword(Keyword::BETWEEN) => {
                self.between_depth = Some(self.paren_depth as i32);
                self.emit_default_token();
            }

            // ── Default ───────────────────────────────────────────────────────
            _ => {
                self.emit_default_token();
            }
        }
    }

    // ── SELECT list handling ────────────────────────────────────────────────────

    fn handle_list_comma(&mut self) {
        match self.config.comma_style {
            CommaStyle::Trailing => {
                self.emit_current_token();
                self.needs_line = true;
            }
            CommaStyle::Leading => {
                self.needs_line = true;
                self.flush_pending_line();
                self.emit_current_token();
                self.emit_space();
            }
        }
    }

    // ── CREATE TABLE handling ───────────────────────────────────────────────────

    fn in_create_table_body(&self) -> bool {
        self.indent_stack.last() == Some(&IndentKind::CreateTableBody)
    }

    fn handle_create_table(&mut self) {
        // Emit TABLE keyword and table name using default token emission
        // until we hit LParen, then enter CreateTableBody mode
        while self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos].token;
            match token {
                Token::LParen => {
                    self.emit_current_token();
                    self.paren_depth += 1;
                    self.pos += 1;
                    self.indent_stack.push(IndentKind::CreateTableBody);
                    self.needs_line = true;
                    return;
                }
                Token::Eof => return,
                _ => {
                    self.emit_default_token();
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn format_sql(input: &str) -> String {
        let tokens = crate::Tokenizer::new(input).preserve_comments(true).tokenize().unwrap();
        TokenFormatter::new(input, tokens).format()
    }

    fn format_sql_with(input: &str, config: FormatConfig) -> String {
        let tokens = crate::Tokenizer::new(input).preserve_comments(true).tokenize().unwrap();
        TokenFormatter::with_config(input, tokens, config).format()
    }

    // ── Config tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_format_config_default() {
        let config = FormatConfig::default();
        assert_eq!(config.indent_width, 2);
        assert_eq!(config.keyword_case, KeywordCase::Preserve);
        assert_eq!(config.comma_style, CommaStyle::Trailing);
        assert_eq!(config.line_width, 120);
        assert!(!config.uppercase_keywords);
        assert!(config.semicolon_newline);
        assert!(config.select_newline);
        assert!(config.logical_operator_newline);
    }

    #[test]
    fn test_format_config_custom() {
        let config = FormatConfig {
            indent_width: 4,
            keyword_case: KeywordCase::Upper,
            comma_style: CommaStyle::Leading,
            line_width: 80,
            ..Default::default()
        };
        assert_eq!(config.indent_width, 4);
        assert_eq!(config.keyword_case, KeywordCase::Upper);
        assert_eq!(config.comma_style, CommaStyle::Leading);
    }

    // ── Backward compat tests (existing) ───────────────────────────────────────

    #[test]
    fn test_simple_select_preserves_content() {
        let input = "SELECT id, name FROM users WHERE id = 1";
        let output = format_sql(input);
        assert_eq!(output.replace(char::is_whitespace, ""), input.replace(char::is_whitespace, ""));
    }

    #[test]
    fn test_preserves_quoted_identifiers() {
        let input = r#"SELECT "BIGFUND"."PKG_BM_2" FROM dual"#;
        let output = format_sql(input);
        assert!(output.contains(r#""BIGFUND""#), "Quoted identifier should stay quoted");
        assert!(output.contains(r#""PKG_BM_2""#), "Quoted identifier should stay quoted");
    }

    #[test]
    fn test_preserves_unquoted_identifiers() {
        let input = "SELECT BIGFUND.PKG_BM_2 FROM dual";
        let output = format_sql(input);
        assert!(output.contains("BIGFUND.PKG_BM_2"), "Unquoted should stay unquoted");
        assert!(!output.contains(r#""BIGFUND""#), "Should NOT add quotes to unquoted identifiers");
    }

    #[test]
    fn test_preserves_single_line_comment() {
        let input = "SELECT -- this is a comment\na FROM t";
        let output = format_sql(input);
        assert!(output.contains("-- this is a comment"), "Single-line comment should be preserved");
    }

    #[test]
    fn test_preserves_block_comment() {
        let input = "SELECT /* block comment */ a FROM t";
        let output = format_sql(input);
        assert!(output.contains("/* block comment */"), "Block comment should be preserved");
    }

    #[test]
    fn test_begin_end_indentation() {
        let input = "BEGIN p_out := 0; END";
        let output = format_sql(input);
        assert!(output.contains("BEGIN\n  p_out := 0;\nEND"), "BEGIN body should be indented, got: {:?}", output);
    }

    #[test]
    fn test_nested_begin_end() {
        let input = "BEGIN BEGIN x := 1; END; END";
        let output = format_sql(input);
        assert!(output.contains("BEGIN\n    x := 1;\n  END"), "Nested block should be doubly indented");
    }

    #[test]
    fn test_exception_block() {
        let input = "BEGIN x := 1; EXCEPTION WHEN OTHERS THEN x := 0; END";
        let output = format_sql(input);
        assert!(output.contains("EXCEPTION\n  WHEN OTHERS THEN\n    x := 0;"));
    }

    #[test]
    fn test_if_then_end_if() {
        let input = "IF x > 0 THEN y := 1; END IF";
        let output = format_sql(input);
        assert!(output.contains("IF x > 0 THEN\n  y := 1;\nEND IF"));
    }

    #[test]
    fn test_loop_end_loop() {
        let input = "LOOP x := x + 1; END LOOP";
        let output = format_sql(input);
        assert!(output.contains("LOOP\n  x := x + 1;\nEND LOOP"));
    }

    #[test]
    fn test_preserves_end_label() {
        let input = "END pkg_batchpay_management_2";
        let output = format_sql(input);
        assert!(output.contains("pkg_batchpay_management_2"), "End label should be preserved");
    }

    #[test]
    fn test_string_literals_preserved() {
        let input = "SELECT 'hello world' FROM dual";
        let output = format_sql(input);
        assert!(output.contains("'hello world'"), "String literal should be preserved exactly");
    }

    #[test]
    fn test_keyword_casing_preserved() {
        let input = "select id from users";
        let config = FormatConfig { keyword_case: KeywordCase::Preserve, ..Default::default() };
        let output = format_sql_with(input, config);
        assert!(output.contains("select"), "Lowercase keyword should stay lowercase");
        assert!(!output.contains("SELECT"), "Should NOT uppercase keywords");
    }

    // ── New config-driven tests ────────────────────────────────────────────────

    #[test]
    fn test_indent_width_4() {
        let config = FormatConfig { indent_width: 4, ..Default::default() };
        let input = "BEGIN x := 1; END";
        let output = format_sql_with(input, config);
        assert!(output.contains("BEGIN\n    x := 1;\nEND"), "4-space indent: {:?}", output);
    }

    #[test]
    fn test_keyword_case_upper() {
        let config = FormatConfig { keyword_case: KeywordCase::Upper, ..Default::default() };
        let input = "select id from users";
        let output = format_sql_with(input, config);
        assert!(output.contains("SELECT"), "Keywords should be uppercase: {:?}", output);
        assert!(!output.contains("select"), "Should not contain lowercase select");
    }

    #[test]
    fn test_keyword_case_lower() {
        let config = FormatConfig { keyword_case: KeywordCase::Lower, ..Default::default() };
        let input = "SELECT id FROM users";
        let output = format_sql_with(input, config);
        assert!(output.contains("select"), "Keywords should be lowercase: {:?}", output);
        assert!(!output.contains("SELECT"), "Should not contain uppercase SELECT");
    }

    #[test]
    fn test_keyword_case_preserves_identifiers() {
        // Issue #311: `name` is in the keyword list (Keyword::NAME_P), so when it
        // appears as a column identifier after a dot (e.g. `a.name`), it must NOT
        // be case-transformed like a real keyword.
        let config_upper = FormatConfig { keyword_case: KeywordCase::Upper, ..Default::default() };
        let input_upper = "SELECT a.id, a.name FROM users";
        let output_upper = format_sql_with(input_upper, config_upper);
        assert!(output_upper.contains("a.name"), "identifier after dot must be preserved: {output_upper:?}");
        assert!(!output_upper.contains("a.NAME"), "identifier after dot must not be uppercased: {output_upper:?}");
        assert!(
            output_upper.contains("SELECT") && output_upper.contains("FROM"),
            "keywords uppercased: {output_upper:?}"
        );

        let config_upper2 = FormatConfig { keyword_case: KeywordCase::Upper, ..Default::default() };
        let input_lower = "select a.id, a.name from users";
        let output_upper2 = format_sql_with(input_lower, config_upper2);
        assert!(output_upper2.contains("a.name"), "identifier after dot must be preserved: {output_upper2:?}");
        assert!(!output_upper2.contains("a.NAME"), "identifier after dot must not be uppercased: {output_upper2:?}");
        assert!(
            output_upper2.contains("SELECT") && output_upper2.contains("FROM"),
            "keywords uppercased: {output_upper2:?}"
        );

        let config_lower = FormatConfig { keyword_case: KeywordCase::Lower, ..Default::default() };
        let input_lower_case = "SELECT a.id, a.name FROM users";
        let output_lower = format_sql_with(input_lower_case, config_lower);
        assert!(output_lower.contains("a.name"), "identifier after dot must be preserved: {output_lower:?}");
        assert!(output_lower.contains("select") && output_lower.contains("from"), "keywords lowered: {output_lower:?}");
    }

    #[test]
    fn test_uppercase_keywords_compat() {
        let config = FormatConfig { uppercase_keywords: true, ..Default::default() };
        let input = "select id from users";
        let output = format_sql_with(input, config);
        assert!(output.contains("SELECT"), "uppercase_keywords should force uppercase");
    }

    // ── SELECT list formatting ─────────────────────────────────────────────────

    #[test]
    fn test_select_columns_each_on_new_line() {
        let config = FormatConfig { select_newline: true, ..Default::default() };
        let input = "SELECT id, name, age FROM users WHERE id = 1";
        let output = format_sql_with(input, config);
        assert!(output.contains("SELECT\n  id,\n  name,\n  age\nFROM"), "Columns on new lines: {:?}", output);
    }

    #[test]
    fn test_select_columns_inline() {
        let config = FormatConfig { select_newline: false, ..Default::default() };
        let input = "SELECT id, name FROM users";
        let output = format_sql_with(input, config);
        let compact: String = output.chars().filter(|c| !c.is_whitespace()).collect();
        let input_compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(compact, input_compact);
    }

    // ── AND/OR formatting ──────────────────────────────────────────────────────

    #[test]
    fn test_where_and_or_newline() {
        let config = FormatConfig { logical_operator_newline: true, select_newline: false, ..Default::default() };
        let input = "SELECT id FROM users WHERE a = 1 AND b = 2 OR c = 3";
        let output = format_sql_with(input, config);
        assert!(output.contains("WHERE a = 1\n  AND b = 2\n  OR c = 3"), "AND/OR on new lines: {:?}", output);
    }

    #[test]
    fn test_where_and_or_inline() {
        let config = FormatConfig { logical_operator_newline: false, select_newline: false, ..Default::default() };
        let input = "SELECT id FROM users WHERE a = 1 AND b = 2";
        let output = format_sql_with(input, config);
        assert!(output.contains("WHERE a = 1 AND b = 2"), "AND/OR inline: {:?}", output);
    }

    // ── Comma position ─────────────────────────────────────────────────────────

    #[test]
    fn test_comma_trailing() {
        let config = FormatConfig { comma_style: CommaStyle::Trailing, select_newline: true, ..Default::default() };
        let input = "SELECT id, name, age FROM t";
        let output = format_sql_with(input, config);
        assert!(output.contains("id,\n  name,\n  age"), "Trailing comma: {:?}", output);
    }

    #[test]
    fn test_comma_leading() {
        let config = FormatConfig { comma_style: CommaStyle::Leading, select_newline: true, ..Default::default() };
        let input = "SELECT id, name, age FROM t";
        let output = format_sql_with(input, config);
        assert!(output.contains("id\n  , name\n  , age"), "Leading comma: {:?}", output);
    }

    // ── CREATE TABLE formatting ────────────────────────────────────────────────

    #[test]
    fn test_create_table_columns() {
        let input = "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL, age INTEGER)";
        let output = format_sql(input);
        assert!(output.contains("(\n  id INTEGER PRIMARY KEY"), "First column: {:?}", output);
        assert!(output.contains(",\n  name VARCHAR(100) NOT NULL"), "Second column: {:?}", output);
        assert!(output.contains(",\n  age INTEGER\n)"), "Last column: {:?}", output);
    }

    #[test]
    fn test_create_table_with_constraints() {
        let input = "CREATE TABLE t (id INT, CONSTRAINT pk_t PRIMARY KEY (id), FOREIGN KEY (id) REFERENCES other(id))";
        let output = format_sql(input);
        assert!(output.contains("CONSTRAINT pk_t"), "Constraint preserved: {:?}", output);
    }

    // ── Subquery indentation ───────────────────────────────────────────────────

    #[test]
    fn test_subquery_indentation() {
        let input = "SELECT * FROM (SELECT id FROM users WHERE active = 1) AS subq";
        let output = format_sql(input);
        // Subquery should be indented inside parens
        let compact: String = output.chars().filter(|c| !c.is_whitespace()).collect();
        let input_compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(compact, input_compact, "Content preserved: {:?}", output);
    }

    // ── JOIN formatting ────────────────────────────────────────────────────────

    #[test]
    fn test_join_formatting() {
        let input =
            "SELECT a.id FROM users a JOIN orders b ON a.id = b.user_id LEFT JOIN products c ON b.product_id = c.id";
        let output = format_sql(input);
        assert!(output.contains("JOIN orders"), "JOIN preserved: {:?}", output);
        assert!(output.contains("ON a.id = b.user_id"), "ON condition preserved: {:?}", output);
        assert!(output.contains("LEFT"), "LEFT JOIN preserved: {:?}", output);
        assert!(output.contains("JOIN products"), "JOIN products: {:?}", output);
    }

    // ── CREATE FUNCTION/PROCEDURE formatting ───────────────────────────────────

    #[test]
    fn test_create_function_formatting() {
        let input = "CREATE OR REPLACE FUNCTION my_func(p1 INTEGER) RETURNS INTEGER IS BEGIN RETURN p1 + 1; END";
        let output = format_sql(input);
        assert!(output.contains("FUNCTION my_func"), "Function header: {:?}", output);
        assert!(output.contains("IS\nBEGIN"), "IS -> BEGIN: {:?}", output);
        assert!(output.contains("RETURN p1 + 1;"), "Return statement: {:?}", output);
    }

    // ── PACKAGE BODY formatting (issue #304) ────────────────────────────────────

    #[test]
    fn test_plsql_package_body_formatting() {
        let input = "CREATE OR REPLACE PACKAGE BODY test_pkg IS\n\
PROCEDURE do_something(p_id IN NUMBER) IS\n\
BEGIN\n\
  INSERT INTO log_table VALUES(p_id, now());\n\
  COMMIT;\n\
END;\n\
FUNCTION get_name(p_id IN NUMBER) RETURN VARCHAR2 IS\n\
BEGIN\n\
  RETURN 'name';\n\
END;\n\
END test_pkg;";
        let output = format_sql(input);

        // 1. No leading whitespace on first line.
        assert!(
            !output.starts_with(' ') && !output.starts_with('\n'),
            "First line must not have leading whitespace: {:?}",
            output
        );

        // 2. PROCEDURE/FUNCTION indented inside PACKAGE BODY (2 spaces).
        assert!(
            output.contains("\n  PROCEDURE do_something"),
            "PROCEDURE should be indented inside PACKAGE BODY: {:?}",
            output
        );
        assert!(
            output.contains("\n  FUNCTION get_name"),
            "FUNCTION should be indented inside PACKAGE BODY: {:?}",
            output
        );

        // 3. Blank line between PROCEDURE and FUNCTION definitions.
        assert!(
            output.contains("\n\n  FUNCTION get_name"),
            "Expected blank line before FUNCTION following PROCEDURE END;: {:?}",
            output
        );

        // 4. RETURN has a preceding space (not `)RETURN`).
        assert!(!output.contains(")RETURN"), "Missing space before RETURN: {:?}", output);
        assert!(output.contains(") RETURN VARCHAR2"), "Space before RETURN VARCHAR2: {:?}", output);

        // BEGIN/END contents still indented as before.
        assert!(output.contains("INSERT INTO log_table"), "INSERT preserved: {:?}", output);
        assert!(output.contains("COMMIT;"), "COMMIT preserved: {:?}", output);
        assert!(output.contains("RETURN 'name';"), "RETURN 'name' preserved: {:?}", output);
    }

    // ── PL/SQL formatting options (issue #305, Phase 1) ─────────────────────────

    #[test]
    fn test_format_config_plsql_defaults() {
        let config = FormatConfig::default();
        assert_eq!(config.block_indent, None);
        assert!(!config.preserve_blank_lines);
        assert!(config.procedure_newline);
    }

    const PLSQL_PACKAGE_BODY_INPUT: &str = "CREATE OR REPLACE PACKAGE BODY test_pkg IS\n\
PROCEDURE do_something(p_id IN NUMBER) IS\n\
BEGIN\n\
  INSERT INTO log_table VALUES(p_id, now());\n\
  COMMIT;\n\
END;\n\
FUNCTION get_name(p_id IN NUMBER) RETURN VARCHAR2 IS\n\
BEGIN\n\
  RETURN 'name';\n\
END;\n\
END test_pkg;";

    #[test]
    fn test_block_indent_overrides_pl_block_indentation() {
        let config = FormatConfig { block_indent: Some(4), ..Default::default() };
        let output = format_sql_with(PLSQL_PACKAGE_BODY_INPUT, config);

        // PROCEDURE/FUNCTION are one level deep inside PACKAGE BODY -> 4 spaces.
        assert!(
            output.contains("\n    PROCEDURE do_something"),
            "PROCEDURE should use block_indent (4 spaces): {:?}",
            output
        );
        assert!(
            output.contains("\n    FUNCTION get_name"),
            "FUNCTION should use block_indent (4 spaces): {:?}",
            output
        );

        // Statements inside BEGIN...END are two levels deep (Package + Begin) -> 8 spaces.
        assert!(
            output.contains("\n        INSERT INTO log_table"),
            "INSERT inside BEGIN should use 2x block_indent (8 spaces): {:?}",
            output
        );
        assert!(
            output.contains("\n        RETURN 'name';"),
            "RETURN inside BEGIN should use 2x block_indent (8 spaces): {:?}",
            output
        );
    }

    #[test]
    fn test_procedure_newline_false_suppresses_blank_line() {
        let config = FormatConfig { procedure_newline: false, ..Default::default() };
        let output = format_sql_with(PLSQL_PACKAGE_BODY_INPUT, config);

        assert!(
            !output.contains("\n\n  FUNCTION get_name"),
            "No blank line should precede FUNCTION when procedure_newline=false: {:?}",
            output
        );
        assert!(
            output.contains("END;\n  FUNCTION get_name"),
            "FUNCTION should still start on its own line: {:?}",
            output
        );
    }

    #[test]
    fn test_preserve_blank_lines_keeps_blank_line_between_statements() {
        let input = "SELECT a FROM t;\n\nSELECT b FROM t2;";

        let preserved = format_sql_with(
            input,
            FormatConfig { preserve_blank_lines: true, select_newline: false, ..Default::default() },
        );
        assert!(
            preserved.contains("t;\n\nSELECT b"),
            "Blank line between statements should be preserved: {:?}",
            preserved
        );

        let collapsed = format_sql_with(
            input,
            FormatConfig { preserve_blank_lines: false, select_newline: false, ..Default::default() },
        );
        assert!(
            !collapsed.contains("t;\n\nSELECT b"),
            "Without preserve_blank_lines, blank line should be collapsed: {:?}",
            collapsed
        );
    }

    // ── PL/pgSQL enhanced ──────────────────────────────────────────────────────

    #[test]
    fn test_case_expression() {
        let input = "CASE WHEN x > 0 THEN 'positive' WHEN x < 0 THEN 'negative' ELSE 'zero' END";
        let output = format_sql(input);
        assert!(output.contains("CASE"), "CASE preserved: {:?}", output);
        assert!(output.contains("END"), "END preserved: {:?}", output);
    }

    #[test]
    fn test_while_loop() {
        let input = "BEGIN WHILE x > 0 LOOP x := x - 1; END LOOP; END";
        let output = format_sql(input);
        assert!(output.contains("WHILE x > 0"), "WHILE header: {:?}", output);
        assert!(output.contains("LOOP"), "LOOP keyword: {:?}", output);
        assert!(output.contains("x := x - 1;"), "Loop body: {:?}", output);
    }

    #[test]
    fn test_for_loop() {
        let input = "BEGIN FOR i IN 1..10 LOOP x := x + i; END LOOP; END";
        let output = format_sql(input);
        assert!(output.contains("FOR i IN 1"), "FOR header start: {:?}", output);
        assert!(output.contains("10"), "FOR header end: {:?}", output);
        assert!(output.contains("x := x + i;"), "Loop body: {:?}", output);
    }

    #[test]
    fn test_return_in_block() {
        let input = "BEGIN x := 1; RETURN x; END";
        let output = format_sql(input);
        assert!(output.contains("RETURN x;"), "RETURN in block: {:?}", output);
    }

    // ── Line width ─────────────────────────────────────────────────────────────

    #[test]
    fn test_line_width_unlimited() {
        let config = FormatConfig { line_width: 0, select_newline: false, ..Default::default() };
        let input = "SELECT a, b, c, d, e, f, g, h, i, j FROM very_long_table_name";
        let output = format_sql_with(input, config);
        let compact: String = output.chars().filter(|c| *c != '\n').collect();
        assert!(compact.len() > 50, "Should not wrap when line_width=0");
    }

    // ── INSERT formatting ────────────────────────────────────────────────────

    #[test]
    fn test_insert_columns_and_values() {
        let input = "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30)";
        let output = format_sql(input);
        assert!(output.contains("INSERT INTO users"), "INSERT header: {:?}", output);
        assert!(output.contains("VALUES"), "VALUES keyword: {:?}", output);
        let compact: String = output.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect();
        let input_compact: String = input.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect();
        assert_eq!(compact, input_compact, "Content preserved: {:?}", output);
    }

    #[test]
    fn test_insert_mybatis_params() {
        let tokens =
            crate::Tokenizer::new("INSERT INTO t (a, b) VALUES (#{x}, #{y})").mybatis_params(true).tokenize().unwrap();
        let output = TokenFormatter::new("INSERT INTO t (a, b) VALUES (#{x}, #{y})", tokens).format();
        assert!(output.contains("#{x}"), "MyBatis param preserved: {:?}", output);
        assert!(output.contains("#{y}"), "MyBatis param preserved: {:?}", output);
    }

    // ── DELETE formatting ────────────────────────────────────────────────────

    #[test]
    fn test_delete_from_where() {
        let input = "DELETE FROM users WHERE id = 1";
        let output = format_sql(input);
        assert!(output.contains("DELETE FROM users"), "DELETE FROM same line: {:?}", output);
        assert!(output.contains("WHERE id = 1"), "WHERE clause: {:?}", output);
    }

    #[test]
    fn test_delete_with_and() {
        let input = "DELETE FROM users WHERE id = 1 AND name = 'test'";
        let output = format_sql(input);
        assert!(output.contains("AND"), "AND preserved: {:?}", output);
    }

    // ── UPDATE formatting ────────────────────────────────────────────────────

    #[test]
    fn test_update_set_where() {
        let input = "UPDATE users SET name = 'Bob', age = 25 WHERE id = 1";
        let output = format_sql(input);
        assert!(output.contains("UPDATE users"), "UPDATE header: {:?}", output);
        assert!(output.contains("SET"), "SET keyword: {:?}", output);
        assert!(output.contains("name = 'Bob'"), "Assignment 1: {:?}", output);
        assert!(output.contains("age = 25"), "Assignment 2: {:?}", output);
        assert!(output.contains("WHERE id = 1"), "WHERE clause: {:?}", output);
    }

    #[test]
    fn test_update_single_column() {
        let input = "UPDATE t SET x = 1";
        let output = format_sql(input);
        assert!(output.contains("x = 1"), "Assignment: {:?}", output);
    }

    // ── MERGE formatting ─────────────────────────────────────────────────────

    #[test]
    fn test_merge_matched() {
        let input = "MERGE INTO target t USING source s ON t.id = s.id WHEN MATCHED THEN UPDATE SET t.name = s.name";
        let output = format_sql(input);
        assert!(output.contains("MERGE INTO target t"), "MERGE header: {:?}", output);
        assert!(output.contains("USING source s"), "USING clause: {:?}", output);
        assert!(output.contains("ON t.id = s.id"), "ON condition: {:?}", output);
        assert!(output.contains("WHEN MATCHED THEN"), "WHEN MATCHED: {:?}", output);
        assert!(output.contains("t.name = s.name"), "SET assignment: {:?}", output);
    }

    #[test]
    fn test_merge_not_matched() {
        let input = "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN INSERT (id) VALUES (s.id)";
        let output = format_sql(input);
        assert!(output.contains("WHEN NOT MATCHED THEN"), "WHEN NOT MATCHED: {:?}", output);
        assert!(output.contains("INSERT"), "INSERT keyword: {:?}", output);
        assert!(output.contains("s.id"), "Value preserved: {:?}", output);
    }

    // ── WITH (CTE) formatting ────────────────────────────────────────────────

    #[test]
    fn test_with_cte() {
        let input = "WITH cte AS (SELECT id FROM users) SELECT * FROM cte WHERE id > 10";
        let output = format_sql(input);
        assert!(output.contains("WITH cte AS"), "CTE header: {:?}", output);
        assert!(output.contains("SELECT"), "Select keyword: {:?}", output);
        assert!(output.contains("*"), "Star: {:?}", output);
        assert!(output.contains("FROM cte"), "FROM cte: {:?}", output);
        assert!(output.contains("WHERE id > 10"), "WHERE clause: {:?}", output);
    }

    #[test]
    fn test_with_multiple_ctes() {
        let input = "WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a JOIN b ON a.id = b.id";
        let output = format_sql(input);
        assert!(output.contains("WITH a AS"), "First CTE: {:?}", output);
        assert!(output.contains("b AS"), "Second CTE: {:?}", output);
        assert!(output.contains("JOIN b"), "JOIN preserved: {:?}", output);
    }
}
