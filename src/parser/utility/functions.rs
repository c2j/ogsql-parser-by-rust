// A3 exception: ast is a pure data module; this file references 170+ AST types.
use crate::ast::*;
use crate::parser::{Parser, ParserError};
use crate::token::keyword::Keyword;
use crate::token::Token;

impl Parser {
    pub(crate) fn parse_create_function(&mut self) -> Result<CreateFunctionStatement, ParserError> {
        let name = self.parse_object_name()?;

        let mut parameters = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let param = self.parse_function_parameter()?;
                    parameters.push(param);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        let return_type = if self.match_keyword(Keyword::RETURNS) || self.match_keyword(Keyword::RETURN) {
            self.advance();
            Some(self.parse_type_name()?)
        } else {
            None
        };

        let has_body = if self.match_keyword(Keyword::IS) || self.match_keyword(Keyword::AS) {
            self.advance();
            true
        } else {
            false
        };

        let (block, options) = if has_body {
            if matches!(self.peek(), Token::DollarString { .. }) {
                if let Token::DollarString { body: inner, .. } = self.peek().clone() {
                    self.advance();
                    let block = Self::parse_pl_block_from_str(&inner).ok();
                    let opts = self.parse_function_options();
                    (block, opts)
                } else {
                    unreachable!()
                }
            } else {
                let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                let block = self.parse_procedure_body(&param_names)?;
                (
                    Some(block),
                    FunctionOptions {
                        language: None,
                        volatility: None,
                        strict: None,
                        cost: None,
                        rows: None,
                        leakproof: None,
                        security: None,
                        parallel: None,
                        fenced: None,
                        shippable: None,
                        extra: String::new(),
                    },
                )
            }
        } else {
            let options = self.parse_function_options();
            (None, options)
        };

        Ok(CreateFunctionStatement { replace: false, name, parameters, return_type, options, block })
    }

    fn parse_function_parameter(&mut self) -> Result<RoutineParam, ParserError> {
        // Try mode-first syntax: OUT result INT, IN name VARCHAR2, INOUT val INT, IN OUT val INT
        let saved_pos = self.pos;
        let saved_error_count = self.errors.len();
        if let Some(mode) = self.parse_param_mode() {
            // Mode-first: MODE name type [DEFAULT expr]
            // Check that the next token looks like an identifier/name (not a comma/paren/EOF)
            match self.peek() {
                Token::Ident(_) | Token::QuotedIdent(_) | Token::Keyword(_) => {
                    let name = self.parse_identifier()?;
                    let data_type = self.parse_param_data_type()?;
                    let default_value = self.parse_param_default()?;
                    return Ok(RoutineParam { name, mode: Some(mode), data_type, default_value });
                }
                _ => {
                    // Not a name after mode keyword — restore and try name-first
                    self.pos = saved_pos;
                    self.errors.truncate(saved_error_count);
                }
            }
        } else {
            // No mode found — restore position for name-first attempt
            self.pos = saved_pos;
        }

        // Name-first syntax: name [MODE] type [DEFAULT expr]
        let name = self.parse_identifier()?;
        let mode = self.parse_param_mode();
        let data_type = self.parse_param_data_type()?;
        let default_value = self.parse_param_default()?;

        Ok(RoutineParam { name, mode, data_type, default_value })
    }

    fn parse_param_mode(&mut self) -> Option<String> {
        if self.match_keyword(Keyword::INOUT) {
            self.advance();
            return Some("INOUT".to_string());
        }
        if self.match_keyword(Keyword::IN_P) {
            self.advance();
            if self.match_keyword(Keyword::OUT_P) {
                self.advance();
                return Some("IN OUT".to_string());
            }
            return Some("IN".to_string());
        }
        if self.match_keyword(Keyword::OUT_P) {
            self.advance();
            return Some("OUT".to_string());
        }
        None
    }

    fn parse_param_data_type(&mut self) -> Result<String, ParserError> {
        let mut type_name = String::new();
        let mut depth = 0i32;

        loop {
            match self.peek() {
                Token::Comma | Token::RParen if depth == 0 => break,
                Token::Keyword(Keyword::DEFAULT) if depth == 0 => break,
                Token::ColonEquals if depth == 0 => break,
                Token::LParen => {
                    depth += 1;
                    type_name.push('(');
                    self.advance();
                }
                Token::RParen => {
                    depth -= 1;
                    type_name.push(')');
                    self.advance();
                }
                Token::Comma => {
                    type_name.push(',');
                    self.advance();
                }
                Token::Dot => {
                    type_name.push('.');
                    self.advance();
                }
                Token::LBracket => {
                    type_name.push('[');
                    self.advance();
                    let mut bracket_depth = 1i32;
                    while bracket_depth > 0 {
                        match self.peek() {
                            Token::LBracket => {
                                bracket_depth += 1;
                                type_name.push('[');
                                self.advance();
                            }
                            Token::RBracket => {
                                bracket_depth -= 1;
                                type_name.push(']');
                                self.advance();
                            }
                            _ => {
                                type_name.push_str(&self.token_to_string());
                                self.advance();
                            }
                        }
                    }
                }
                Token::Percent => {
                    type_name.push('%');
                    self.advance();
                }
                _ => {
                    let tok_str = self.token_to_string();
                    if !type_name.is_empty()
                        && !type_name.ends_with('(')
                        && !type_name.ends_with('[')
                        && !type_name.ends_with('.')
                        && !type_name.ends_with('%')
                        && !type_name.ends_with(',')
                    {
                        type_name.push(' ');
                    }
                    type_name.push_str(&tok_str);
                    self.advance();
                }
            }
        }

        Ok(type_name.trim().to_string())
    }

    fn parse_param_default(&mut self) -> Result<Option<String>, ParserError> {
        let has_default = if self.match_keyword(Keyword::DEFAULT) {
            self.advance();
            true
        } else if matches!(self.peek(), Token::ColonEquals) {
            self.advance();
            true
        } else {
            false
        };

        if !has_default {
            return Ok(None);
        }

        let mut default_val = String::new();
        let mut depth = 0i32;

        loop {
            match self.peek() {
                Token::Comma | Token::RParen if depth == 0 => break,
                Token::LParen => {
                    depth += 1;
                    default_val.push('(');
                    self.advance();
                }
                Token::RParen => {
                    depth -= 1;
                    default_val.push(')');
                    self.advance();
                }
                _ => {
                    let tok_str = self.token_to_string();
                    if !default_val.is_empty() && !default_val.ends_with('(') {
                        default_val.push(' ');
                    }
                    default_val.push_str(&tok_str);
                    self.advance();
                }
            }
        }

        Ok(Some(default_val.trim().to_string()))
    }

    fn parse_type_name(&mut self) -> Result<String, ParserError> {
        let mut type_name = String::new();

        loop {
            match self.peek() {
                Token::Keyword(Keyword::AS) | Token::Keyword(Keyword::IS) => break,
                Token::Ident(s) => {
                    if !type_name.is_empty() {
                        type_name.push(' ');
                    }
                    type_name.push_str(s);
                    self.advance();
                }
                Token::Keyword(kw) => {
                    if !type_name.is_empty() {
                        type_name.push(' ');
                    }
                    type_name.push_str(kw.as_str());
                    self.advance();
                }
                Token::LParen => {
                    type_name.push('(');
                    self.advance();
                    let mut depth = 1;
                    while depth > 0 {
                        match self.peek() {
                            Token::LParen => {
                                depth += 1;
                                type_name.push('(');
                                self.advance();
                            }
                            Token::RParen => {
                                depth -= 1;
                                type_name.push(')');
                                self.advance();
                            }
                            Token::Comma => {
                                type_name.push_str(", ");
                                self.advance();
                            }
                            _ => {
                                type_name.push_str(&self.token_to_string());
                                self.advance();
                            }
                        }
                    }
                }
                Token::Dot => {
                    type_name.push('.');
                    self.advance();
                }
                Token::Percent => {
                    type_name.push('%');
                    self.advance();
                }
                Token::LBracket => {
                    type_name.push('[');
                    self.advance();
                    let mut depth = 1;
                    while depth > 0 {
                        match self.peek() {
                            Token::LBracket => {
                                depth += 1;
                                type_name.push('[');
                                self.advance();
                            }
                            Token::RBracket => {
                                depth -= 1;
                                type_name.push(']');
                                self.advance();
                            }
                            _ => {
                                type_name.push_str(&self.token_to_string());
                                self.advance();
                            }
                        }
                    }
                }
                _ => break,
            }
        }

        Ok(type_name)
    }

    pub(crate) fn token_to_string(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow;
        match self.peek() {
            Token::Ident(s) => Cow::Owned(s.clone()),
            Token::QuotedIdent(s) => Cow::Owned(format!("\"{}\"", s)),
            Token::Keyword(kw) => Cow::Borrowed(kw.as_str()),
            Token::Integer(i) => Cow::Owned(i.to_string()),
            Token::Float(f) => Cow::Owned(f.clone()),
            Token::StringLiteral(s) => Cow::Owned(format!("'{}'", s)),
            Token::EscapeString(s) => Cow::Owned(format!("E'{}'", s)),
            Token::DollarString { body, .. } => Cow::Owned(format!("$$ {} $$", body)),
            Token::LParen => Cow::Borrowed("("),
            Token::RParen => Cow::Borrowed(")"),
            Token::LBracket => Cow::Borrowed("["),
            Token::RBracket => Cow::Borrowed("]"),
            Token::Comma => Cow::Borrowed(","),
            Token::Dot => Cow::Borrowed("."),
            Token::Semicolon => Cow::Borrowed(";"),
            Token::Colon => Cow::Borrowed(":"),
            Token::ColonEquals => Cow::Borrowed(":="),
            Token::ParamEquals => Cow::Borrowed("=>"),
            Token::Op(s) => Cow::Owned(s.clone()),
            Token::OpLe => Cow::Borrowed("<="),
            Token::OpNe => Cow::Borrowed("<>"),
            Token::OpGe => Cow::Borrowed(">="),
            Token::OpShiftL => Cow::Borrowed("<<"),
            Token::OpShiftR => Cow::Borrowed(">>"),
            Token::OpArrow => Cow::Borrowed("->"),
            Token::OpJsonArrow => Cow::Borrowed("->>"),
            Token::OpNe2 => Cow::Borrowed("!="),
            Token::OpDblBang => Cow::Borrowed("!!"),
            Token::OpConcat => Cow::Borrowed("||"),
            Token::Param(n) => Cow::Owned(format!("${}", n)),
            Token::MyBatisParam(s) => Cow::Owned(format!("#{{{}}}", s)),
            Token::MyBatisRawExpr(s) => Cow::Owned(format!("${{{}}}", s)),
            Token::JdbcParam => Cow::Borrowed("?"),
            Token::Star => Cow::Borrowed("*"),
            Token::Eq => Cow::Borrowed("="),
            Token::Plus => Cow::Borrowed("+"),
            Token::Minus => Cow::Borrowed("-"),
            Token::Lt => Cow::Borrowed("<"),
            Token::Gt => Cow::Borrowed(">"),
            Token::Percent => Cow::Borrowed("%"),
            Token::Slash => Cow::Borrowed("/"),
            Token::Caret => Cow::Borrowed("^"),
            Token::At => Cow::Borrowed("@"),
            Token::Typecast => Cow::Borrowed("::"),
            Token::DotDot => Cow::Borrowed(".."),
            Token::PlusJoin => Cow::Borrowed("(+)"),
            Token::BitString(s) => Cow::Owned(format!("B'{}'", s)),
            Token::HexString(s) => Cow::Owned(format!("X'{}'", s)),
            Token::NationalString(s) => Cow::Owned(format!("N'{}'", s)),
            Token::SetIdent(s) => Cow::Owned(format!("@@{}", s)),
            Token::Comment(s) => Cow::Owned(s.clone()),
            Token::Eof => Cow::Borrowed(""),
            Token::Hint(h) => Cow::Owned(format!("/*+ {} */", h)),
        }
    }

    fn parse_function_options(&mut self) -> FunctionOptions {
        let raw = self.skip_to_semicolon_and_collect();
        Self::parse_function_options_from_raw(&raw)
    }

    fn parse_function_options_from_raw(raw: &str) -> FunctionOptions {
        let mut opts = FunctionOptions {
            language: None,
            volatility: None,
            strict: None,
            cost: None,
            rows: None,
            leakproof: None,
            security: None,
            parallel: None,
            fenced: None,
            shippable: None,
            extra: String::new(),
        };
        let parts: Vec<&str> = raw.split_whitespace().collect();
        let mut i = 0;
        let mut extra_parts = Vec::new();
        while i < parts.len() {
            match parts[i].to_uppercase().as_str() {
                "LANGUAGE" if i + 1 < parts.len() => {
                    opts.language = Some(parts[i + 1].to_string());
                    i += 2;
                }
                "IMMUTABLE" => {
                    opts.volatility = Some(Volatility::Immutable);
                    i += 1;
                }
                "STABLE" => {
                    opts.volatility = Some(Volatility::Stable);
                    i += 1;
                }
                "VOLATILE" => {
                    opts.volatility = Some(Volatility::Volatile);
                    i += 1;
                }
                "STRICT" => {
                    opts.strict = Some(true);
                    i += 1;
                }
                "LEAKPROOF" => {
                    opts.leakproof = Some(true);
                    i += 1;
                }
                "COST" if i + 1 < parts.len() => {
                    if let Ok(n) = parts[i + 1].parse::<u32>() {
                        opts.cost = Some(n);
                        i += 2;
                    } else {
                        extra_parts.push(parts[i]);
                        i += 1;
                    }
                }
                "ROWS" if i + 1 < parts.len() => {
                    if let Ok(n) = parts[i + 1].parse::<u32>() {
                        opts.rows = Some(n);
                        i += 2;
                    } else {
                        extra_parts.push(parts[i]);
                        i += 1;
                    }
                }
                "PARALLEL" if i + 1 < parts.len() => match parts[i + 1].to_uppercase().as_str() {
                    "SAFE" => {
                        opts.parallel = Some(ParallelMode::Safe);
                        i += 2;
                    }
                    "UNSAFE" => {
                        opts.parallel = Some(ParallelMode::Unsafe);
                        i += 2;
                    }
                    "RESTRICTED" => {
                        opts.parallel = Some(ParallelMode::Restricted);
                        i += 2;
                    }
                    _ => {
                        extra_parts.push(parts[i]);
                        i += 1;
                    }
                },
                "SECURITY" if i + 1 < parts.len() => match parts[i + 1].to_uppercase().as_str() {
                    "INVOKER" => {
                        opts.security = Some(SecurityMode::Invoker);
                        i += 2;
                    }
                    "DEFINER" => {
                        opts.security = Some(SecurityMode::Definer);
                        i += 2;
                    }
                    _ => {
                        extra_parts.push(parts[i]);
                        i += 1;
                    }
                },
                "NOT" if i + 1 < parts.len() && parts[i + 1].to_uppercase() == "LEAKPROOF" => {
                    opts.leakproof = Some(false);
                    i += 2;
                }
                "FENCED" => {
                    opts.fenced = Some(true);
                    i += 1;
                }
                "NOT" if i + 1 < parts.len() && parts[i + 1].to_uppercase() == "FENCED" => {
                    opts.fenced = Some(false);
                    i += 2;
                }
                "SHIPPABLE" => {
                    opts.shippable = Some(true);
                    i += 1;
                }
                "NOT" if i + 1 < parts.len() && parts[i + 1].to_uppercase() == "SHIPPABLE" => {
                    opts.shippable = Some(false);
                    i += 2;
                }
                _ => {
                    extra_parts.push(parts[i]);
                    i += 1;
                }
            }
        }
        opts.extra = extra_parts.join(" ");
        opts
    }

    pub(crate) fn skip_to_semicolon_and_collect(&mut self) -> String {
        let mut collected = String::new();
        let mut depth = 0i32;

        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Semicolon if depth == 0 => {
                    self.advance();
                    break;
                }
                Token::LParen => {
                    depth += 1;
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                Token::RParen => {
                    depth -= 1;
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                _ => {
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
            }
        }

        collected.trim().to_string()
    }

    pub(crate) fn parse_create_procedure(&mut self) -> Result<CreateProcedureStatement, ParserError> {
        let name = self.parse_object_name()?;

        let mut parameters = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let param = self.parse_function_parameter()?;
                    parameters.push(param);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        let has_body = if self.match_keyword(Keyword::IS) || self.match_keyword(Keyword::AS) {
            self.advance();
            true
        } else {
            false
        };

        let (block, options) = if has_body {
            if matches!(self.peek(), Token::DollarString { .. }) {
                if let Token::DollarString { body: inner, .. } = self.peek().clone() {
                    self.advance();
                    let block = Self::parse_pl_block_from_str(&inner).ok();
                    let opts = self.parse_function_options();
                    (block, opts)
                } else {
                    unreachable!()
                }
            } else {
                let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                let block = self.parse_procedure_body(&param_names)?;
                (
                    Some(block),
                    FunctionOptions {
                        language: None,
                        volatility: None,
                        strict: None,
                        cost: None,
                        rows: None,
                        leakproof: None,
                        security: None,
                        parallel: None,
                        fenced: None,
                        shippable: None,
                        extra: String::new(),
                    },
                )
            }
        } else if self.procedure_body_after_options() {
            let raw = self.collect_until_body_marker();
            let opts = Self::parse_function_options_from_raw(&raw);
            // At AS/IS marker now.
            self.advance();
            let block = if matches!(self.peek(), Token::DollarString { .. }) {
                if let Token::DollarString { body: inner, .. } = self.peek().clone() {
                    self.advance();
                    Self::parse_pl_block_from_str(&inner).ok()
                } else {
                    unreachable!()
                }
            } else {
                let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                Some(self.parse_procedure_body(&param_names)?)
            };
            (block, opts)
        } else {
            let options = self.parse_function_options();
            (None, options)
        };

        Ok(CreateProcedureStatement { replace: false, name, parameters, options, block })
    }

    /// Detect `... OPTIONS ... AS/IS body` where options appear between the
    /// parameter list and the body marker (e.g. `LANGUAGE plpgsql AS $$...$$`).
    /// Only claims bodies this parser can actually consume (dollar-quoted or an
    /// inline DECLARE/BEGIN block); other forms such as `AS '<string>'` are left
    /// to the options-only path so they keep parsing as before.
    fn procedure_body_after_options(&self) -> bool {
        let mut i = self.pos;
        let mut depth = 0i32;
        while i < self.tokens.len() {
            match &self.tokens[i].token {
                Token::Keyword(Keyword::IS) | Token::Keyword(Keyword::AS) if depth == 0 => {
                    return matches!(
                        self.tokens.get(i + 1).map(|t| &t.token),
                        Some(Token::DollarString { .. })
                            | Some(Token::Keyword(Keyword::BEGIN_P))
                            | Some(Token::Keyword(Keyword::DECLARE))
                    );
                }
                Token::Semicolon if depth == 0 => return false,
                Token::Eof => return false,
                Token::LParen => depth += 1,
                Token::RParen => depth = (depth - 1).max(0),
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Collect raw token text up to (but not consuming) the AS/IS body marker.
    fn collect_until_body_marker(&mut self) -> String {
        let mut collected = String::new();
        let mut depth = 0i32;
        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Keyword(Keyword::IS) | Token::Keyword(Keyword::AS) if depth == 0 => break,
                Token::Semicolon if depth == 0 => break,
                Token::LParen => {
                    depth += 1;
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                Token::RParen => {
                    depth -= 1;
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                _ => {
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
            }
        }
        collected.trim().to_string()
    }

    pub(crate) fn parse_create_package(&mut self, replace: bool) -> Result<Statement, ParserError> {
        self.expect_keyword(Keyword::PACKAGE)?;
        let is_body = if self.match_keyword(Keyword::BODY_P) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.parse_object_name()?;

        let authid = if !is_body && self.match_keyword(Keyword::AUTHID) {
            self.advance();
            if self.match_keyword(Keyword::CURRENT_USER) {
                self.advance();
                Some(PackageAuthid::CurrentUser)
            } else {
                self.try_consume_keyword(Keyword::DEFINER);
                Some(PackageAuthid::Definer)
            }
        } else {
            None
        };

        if self.match_keyword(Keyword::AS) || self.match_keyword(Keyword::IS) {
            self.advance();
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "AS or IS".to_string(),
                got: format!("{:?}", self.peek()),
            });
        }

        let items = self.parse_package_body_items();
        if is_body {
            Ok(Statement::CreatePackageBody(crate::ast::Spanned::new(
                CreatePackageBodyStatement { replace, name, items },
                None,
            )))
        } else {
            Ok(Statement::CreatePackage(crate::ast::Spanned::new(
                CreatePackageStatement { replace, name, authid, items },
                None,
            )))
        }
    }

    /// Check if the PROCEDURE/FUNCTION token at current pos is followed by
    /// name, optional params, then IS/AS before semicolon (definition with body)
    /// vs just a declaration (semicolon before IS/AS).
    fn is_subprogram_definition_ahead(&self) -> bool {
        let mut i = self.pos + 1;
        let mut paren_d = 0i32;
        while i < self.tokens.len() {
            match &self.tokens[i].token {
                Token::LParen => paren_d += 1,
                Token::RParen => paren_d = (paren_d - 1).max(0),
                Token::Keyword(Keyword::IS) | Token::Keyword(Keyword::AS) if paren_d == 0 => {
                    return true;
                }
                Token::Semicolon if paren_d == 0 => return false,
                Token::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    pub(crate) fn parse_package_body_items(&mut self) -> Vec<PackageItem> {
        let mut items = Vec::new();
        let mut depth = 0i32;

        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Keyword(Keyword::BEGIN_P) => {
                    depth += 1;
                    self.advance();
                }
                Token::Keyword(Keyword::END_P) => {
                    if depth > 0 {
                        depth -= 1;
                        self.advance();
                    } else {
                        self.advance();
                        // END IF / END LOOP / END CASE — compound ends, skip them
                        if self.match_keyword(Keyword::IF_P)
                            || self.match_keyword(Keyword::LOOP)
                            || self.match_keyword(Keyword::CASE)
                        {
                            self.advance();
                            continue;
                        }
                        // END followed by ident and then semicolon/EOF = final END pkg_name;
                        // END followed by comma or other = CASE alias or similar, keep going
                        if matches!(self.peek(), Token::Ident(_) | Token::Keyword(_)) {
                            // Peek further: if the token after the ident is ; or EOF, this is END package_name
                            let after_name = self.tokens.get(self.pos + 1);
                            if after_name.is_none()
                                || matches!(after_name.map(|t| &t.token), Some(Token::Semicolon) | Some(Token::Slash))
                            {
                                while matches!(self.peek(), Token::Ident(_) | Token::Keyword(_)) {
                                    self.advance();
                                }
                                break;
                            }
                            // Otherwise it's something like END check_type, — skip ident and continue
                            self.advance();
                            continue;
                        }
                        break;
                    }
                }
                Token::Keyword(Keyword::PROCEDURE) => {
                    let sp_start = self.pos;
                    self.advance();
                    match self.parse_package_sub_procedure(sp_start) {
                        Ok(proc) => {
                            items.push(PackageItem::Procedure(proc));
                        }
                        Err(e) => {
                            self.add_error(e);
                            let raw = self.skip_to_end_subprogram();
                            if !raw.is_empty() {
                                items.push(PackageItem::Raw(format!("PROCEDURE {}", raw)));
                            }
                        }
                    }
                }
                Token::Keyword(Keyword::FUNCTION) => {
                    let sp_start = self.pos;
                    self.advance();
                    match self.parse_package_sub_function(sp_start) {
                        Ok(func) => {
                            items.push(PackageItem::Function(func));
                        }
                        Err(e) => {
                            self.add_error(e);
                            let raw = self.skip_to_end_subprogram();
                            if !raw.is_empty() {
                                items.push(PackageItem::Raw(format!("FUNCTION {}", raw)));
                            }
                        }
                    }
                }
                Token::Keyword(Keyword::TYPE_P) => {
                    self.advance();
                    match self.parse_identifier() {
                        Ok(type_name) => match self.parse_pl_type_decl_body(type_name) {
                            Ok(crate::ast::plpgsql::PlDeclaration::Type(type_decl)) => {
                                items.push(PackageItem::Type(type_decl));
                            }
                            _ => {
                                self.skip_to_semicolon_or_keyword();
                            }
                        },
                        Err(_) => {
                            self.skip_to_semicolon_or_keyword();
                        }
                    }
                }
                Token::Keyword(Keyword::CURSOR) => {
                    self.advance();
                    match self.parse_identifier() {
                        Ok(cursor_name) => match self.parse_pl_cursor_decl(cursor_name) {
                            Ok(crate::ast::plpgsql::PlDeclaration::Cursor(cursor_decl)) => {
                                items.push(PackageItem::Cursor(cursor_decl));
                            }
                            Err(e) => {
                                self.add_error(e);
                                self.skip_to_semicolon_or_keyword();
                            }
                            _ => {
                                self.skip_to_semicolon_or_keyword();
                            }
                        },
                        Err(_) => {
                            self.skip_to_semicolon_or_keyword();
                        }
                    }
                }
                _ => {
                    if let Some(crate::ast::plpgsql::PlDeclaration::Variable(var)) = self.try_parse_oracle_var_decl() {
                        items.push(PackageItem::Variable(var));
                    } else {
                        self.advance();
                    }
                }
            }
        }

        items
    }

    pub(crate) fn tokens_to_raw_string(&self, start: usize, end: usize) -> String {
        // Fast path: slice from source text using token spans
        if !self.source.is_empty() && start < end && start < self.tokens.len() {
            let byte_start = self.tokens[start].span.start;
            let end_idx = (end - 1).min(self.tokens.len().saturating_sub(1));
            let byte_end = self.tokens[end_idx].span.end;
            let source = &self.source;
            let byte_start = byte_start.min(source.len());
            let byte_end = byte_end.min(source.len());
            if byte_start < byte_end {
                return source[byte_start..byte_end].trim().to_string();
            }
        }
        // Fallback: reconstruct from tokens (used when source is empty)
        self.tokens[start..end]
            .iter()
            .map(|t| match &t.token {
                Token::Ident(s) => s.clone(),
                Token::QuotedIdent(s) => format!("\"{}\"", s),
                Token::Keyword(kw) => kw.as_str().to_string(),
                Token::Integer(i) => i.to_string(),
                Token::Float(f) => f.clone(),
                Token::StringLiteral(s) => format!("'{}'", s),
                Token::EscapeString(s) => format!("E'{}'", s),
                Token::DollarString { body, .. } => format!("$$ {} $$", body),
                Token::LParen => "(".to_string(),
                Token::RParen => ")".to_string(),
                Token::LBracket => "[".to_string(),
                Token::RBracket => "]".to_string(),
                Token::Comma => ",".to_string(),
                Token::Dot => ".".to_string(),
                Token::Semicolon => ";".to_string(),
                Token::Colon => ":".to_string(),
                Token::ColonEquals => ":=".to_string(),
                Token::ParamEquals => "=>".to_string(),
                Token::Op(s) => s.clone(),
                Token::OpLe => "<=".to_string(),
                Token::OpNe => "<>".to_string(),
                Token::OpGe => ">=".to_string(),
                Token::OpShiftL => "<<".to_string(),
                Token::OpShiftR => ">>".to_string(),
                Token::OpArrow => "->".to_string(),
                Token::OpJsonArrow => "->>".to_string(),
                Token::OpNe2 => "!=".to_string(),
                Token::OpDblBang => "!!".to_string(),
                Token::OpConcat => "||".to_string(),
                Token::Param(n) => format!("${}", n),
                Token::MyBatisParam(s) => format!("#{{{}}}", s),
                Token::MyBatisRawExpr(s) => format!("${{{}}}", s),
                Token::JdbcParam => "?".to_string(),
                Token::Star => "*".to_string(),
                Token::Eq => "=".to_string(),
                Token::Plus => "+".to_string(),
                Token::Minus => "-".to_string(),
                Token::Lt => "<".to_string(),
                Token::Gt => ">".to_string(),
                Token::Percent => "%".to_string(),
                Token::Slash => "/".to_string(),
                Token::Caret => "^".to_string(),
                Token::At => "@".to_string(),
                Token::Typecast => "::".to_string(),
                Token::DotDot => "..".to_string(),
                Token::PlusJoin => "(+)".to_string(),
                Token::BitString(s) => format!("B'{}'", s),
                Token::HexString(s) => format!("X'{}'", s),
                Token::NationalString(s) => format!("N'{}'", s),
                Token::SetIdent(s) => format!("@@{}", s),
                Token::Comment(s) => s.clone(),
                Token::Eof => String::new(),
                Token::Hint(h) => format!("/*+ {} */", h),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(crate) fn parse_package_sub_procedure(&mut self, start_pos: usize) -> Result<PackageProcedure, ParserError> {
        let name = self.parse_object_name()?;

        let mut parameters = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let param = self.parse_function_parameter()?;
                    parameters.push(param);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        let has_body = if self.match_keyword(Keyword::IS) || self.match_keyword(Keyword::AS) {
            self.advance();
            true
        } else {
            self.try_consume_semicolon();
            false
        };

        let block = if has_body {
            let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
            Some(self.parse_procedure_body(&param_names)?)
        } else {
            None
        };

        let start_line = self.tokens.get(start_pos).map(|t| t.location.line).unwrap_or(0);
        let end_line = self.pos.saturating_sub(1).min(self.tokens.len().saturating_sub(1));
        let end_line = self.tokens.get(end_line).map(|t| t.location.line).unwrap_or(start_line);

        Ok(PackageProcedure { name, parameters, block, start_line, end_line })
    }

    pub(crate) fn parse_package_sub_function(&mut self, start_pos: usize) -> Result<PackageFunction, ParserError> {
        let name = self.parse_object_name()?;

        let mut parameters = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let param = self.parse_function_parameter()?;
                    parameters.push(param);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        let return_type = if self.match_keyword(Keyword::RETURN) {
            self.advance();
            Some(self.parse_type_name()?)
        } else {
            None
        };

        let has_body = if self.match_keyword(Keyword::IS) || self.match_keyword(Keyword::AS) {
            self.advance();
            true
        } else {
            self.try_consume_semicolon();
            false
        };

        let block = if has_body {
            let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
            Some(self.parse_procedure_body(&param_names)?)
        } else {
            None
        };

        let start_line = self.tokens.get(start_pos).map(|t| t.location.line).unwrap_or(0);
        let end_line = self.pos.saturating_sub(1).min(self.tokens.len().saturating_sub(1));
        let end_line = self.tokens.get(end_line).map(|t| t.location.line).unwrap_or(start_line);

        Ok(PackageFunction { name, parameters, return_type, block, start_line, end_line })
    }

    fn skip_to_end_subprogram(&mut self) -> String {
        let mut collected = String::new();
        let mut begin_depth = 0i32;
        let mut case_depth = 0i32;
        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Keyword(Keyword::CASE) => {
                    case_depth += 1;
                }
                Token::Keyword(Keyword::BEGIN_P) => {
                    begin_depth += 1;
                }
                Token::Keyword(Keyword::END_P) => {
                    if case_depth > 0 {
                        case_depth -= 1;
                    } else if begin_depth > 0 {
                        begin_depth -= 1;
                    } else if !self.lookahead_is_compound_end() {
                        self.advance();
                        while matches!(self.peek(), Token::Ident(_)) {
                            self.advance();
                        }
                        self.try_consume_semicolon();
                        break;
                    }
                }
                _ => {}
            }
            if !collected.is_empty() {
                collected.push(' ');
            }
            collected.push_str(&self.token_to_string());
            self.advance();
        }
        collected.trim().to_string()
    }

    pub(crate) fn parse_create_extension(&mut self) -> Result<CreateExtensionStatement, ParserError> {
        self.expect_keyword(Keyword::EXTENSION)?;
        let if_not_exists = self.parse_if_not_exists();
        let name = self.parse_identifier()?;

        let mut schema = None;
        let mut version = None;
        let mut cascade = false;

        if self.match_keyword(Keyword::WITH) {
            self.advance();
        }
        if self.match_keyword(Keyword::SCHEMA) {
            self.advance();
            schema = Some(self.parse_identifier()?);
        }
        if self.match_ident_str("VERSION") {
            self.advance();
            version = Some(if matches!(self.peek(), Token::StringLiteral(_)) {
                self.parse_string_literal()?
            } else {
                self.parse_identifier()?
            });
        }
        if self.match_keyword(Keyword::CASCADE) {
            self.advance();
            cascade = true;
        }

        Ok(CreateExtensionStatement { replace: false, if_not_exists, name, schema, version, cascade })
    }

    pub(crate) fn parse_create_domain(&mut self) -> Result<CreateDomainStatement, ParserError> {
        self.expect_keyword(Keyword::DOMAIN_P)?;
        let name = self.parse_object_name()?;
        self.try_consume_keyword(Keyword::AS);
        let data_type = self.parse_data_type()?;

        let mut default_value = None;
        let mut not_null = false;
        let mut check = None;

        if self.match_keyword(Keyword::DEFAULT) {
            self.advance();
            default_value = Some(self.parse_expr()?);
        }
        if self.match_keyword(Keyword::NOT) {
            self.advance();
            self.expect_keyword(Keyword::NULL_P)?;
            not_null = true;
        }
        if self.match_keyword(Keyword::CHECK) {
            self.advance();
            self.expect_token(&Token::LParen)?;
            check = Some(self.parse_expr()?);
            self.expect_token(&Token::RParen)?;
        }

        Ok(CreateDomainStatement { name, data_type, default_value, not_null, check })
    }

    pub(crate) fn collect_until_boundary(&mut self, stop_tokens: &[Token]) -> String {
        let mut collected = String::new();
        loop {
            let at_stop = stop_tokens.iter().any(|t| *self.peek() == *t) || *self.peek() == Token::Eof;
            if at_stop {
                break;
            }
            if !collected.is_empty() {
                collected.push(' ');
            }
            collected.push_str(&self.token_to_string());
            self.advance();
        }
        collected.trim().to_string()
    }

    pub(crate) fn collect_until_balanced_paren(&mut self) -> String {
        let mut collected = String::new();
        let mut depth = 1i32;
        loop {
            match self.peek() {
                Token::Eof => break,
                Token::LParen => {
                    depth += 1;
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                Token::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        break;
                    }
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                _ => {
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
            }
        }
        collected.trim().to_string()
    }

    pub(crate) fn parse_create_cast(&mut self) -> Result<CreateCastStatement, ParserError> {
        self.expect_keyword(Keyword::CAST)?;
        self.expect_token(&Token::LParen)?;
        let source_type = self.parse_data_type()?;
        self.expect_keyword(Keyword::AS)?;
        let target_type = self.parse_data_type()?;
        self.expect_token(&Token::RParen)?;

        let method = if self.match_keyword(Keyword::WITHOUT) {
            self.advance();
            self.expect_keyword(Keyword::FUNCTION)?;
            CastMethod::WithoutFunction
        } else if self.match_keyword(Keyword::WITH) {
            self.advance();
            if self.match_keyword(Keyword::INOUT) {
                self.advance();
                CastMethod::WithInout
            } else {
                self.expect_keyword(Keyword::FUNCTION)?;
                let func_name = self.collect_until_boundary(&[Token::Keyword(Keyword::AS), Token::Semicolon]);
                CastMethod::WithFunction(func_name)
            }
        } else {
            CastMethod::WithoutFunction
        };

        let context = if self.match_keyword(Keyword::AS) {
            self.advance();
            if self.match_keyword(Keyword::IMPLICIT_P) {
                self.advance();
                Some(CastContext::Implicit)
            } else {
                self.try_consume_keyword(Keyword::ASSIGNMENT);
                Some(CastContext::Assignment)
            }
        } else {
            None
        };

        Ok(CreateCastStatement { source_type, target_type, method, context })
    }

    fn parse_type_name_for_cast(&mut self) -> Result<String, ParserError> {
        let mut name = String::new();
        loop {
            match self.peek() {
                Token::Keyword(Keyword::AS) => break,
                Token::RParen => break,
                Token::Ident(s) => {
                    if !name.is_empty() {
                        name.push(' ');
                    }
                    name.push_str(s);
                    self.advance();
                }
                Token::Keyword(kw) => {
                    if !name.is_empty() {
                        name.push(' ');
                    }
                    name.push_str(kw.as_str());
                    self.advance();
                }
                Token::LParen => {
                    name.push('(');
                    self.advance();
                    let inner = self.collect_until_balanced_paren();
                    let trimmed = inner.trim();
                    if !trimmed.is_empty() {
                        name.push_str(trimmed);
                    }
                    name.push(')');
                }
                _ => break,
            }
        }
        Ok(name.trim().to_string())
    }

    // ── Wave 6: GRANT / REVOKE ──
}
