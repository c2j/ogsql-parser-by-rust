// A3 exception: ast is a pure data module; this file references 390+ AST types.
use crate::ast::*;
use crate::parser::{Parser, ParserError};
use crate::token::keyword::Keyword;
use crate::token::Token;

impl Parser {
    pub(crate) fn parse_create_trigger(&mut self) -> Result<CreateTriggerStatement, ParserError> {
        let name = self.parse_identifier()?;

        let mut or_replace = false;
        let mut constraint = false;

        if self.match_keyword(Keyword::OR) {
            self.advance();
            if self.try_consume_keyword(Keyword::REPLACE) {
                or_replace = true;
            }
        }

        if self.match_keyword(Keyword::CONSTRAINT) {
            self.advance();
            constraint = true;
        }

        let timing = match self.peek_keyword() {
            Some(Keyword::BEFORE) => {
                self.advance();
                TriggerTiming::Before
            }
            Some(Keyword::AFTER) => {
                self.advance();
                TriggerTiming::After
            }
            Some(Keyword::INSTEAD) => {
                self.advance();
                self.expect_keyword(Keyword::OF)?;
                TriggerTiming::InsteadOf
            }
            _ => {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "BEFORE | AFTER | INSTEAD OF".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
        };

        let mut events = Vec::new();
        loop {
            match self.peek_keyword() {
                Some(Keyword::INSERT) => {
                    self.advance();
                    events.push(TriggerEvent::Insert);
                }
                Some(Keyword::DELETE_P) => {
                    self.advance();
                    events.push(TriggerEvent::Delete);
                }
                Some(Keyword::TRUNCATE) => {
                    self.advance();
                    events.push(TriggerEvent::Truncate);
                }
                Some(Keyword::UPDATE) => {
                    self.advance();
                    if self.match_keyword(Keyword::OF) {
                        // Oracle-style: UPDATE OF col1, col2 (no parens)
                        self.advance();
                        let mut cols = Vec::new();
                        cols.push(self.parse_identifier()?);
                        while self.match_token(&Token::Comma) {
                            self.advance();
                            cols.push(self.parse_identifier()?);
                        }
                        events.push(TriggerEvent::UpdateOf(cols));
                    } else if self.match_token(&Token::LParen) {
                        self.advance();
                        let mut cols = Vec::new();
                        cols.push(self.parse_identifier()?);
                        while self.match_token(&Token::Comma) {
                            self.advance();
                            cols.push(self.parse_identifier()?);
                        }
                        self.expect_token(&Token::RParen)?;
                        events.push(TriggerEvent::UpdateOf(cols));
                    } else {
                        events.push(TriggerEvent::Update);
                    }
                }
                Some(Keyword::OR) => {
                    self.advance();
                    continue;
                }
                _ => break,
            }
        }

        self.expect_keyword(Keyword::ON)?;
        let table = self.parse_object_name()?;

        let for_each = if self.try_consume_keyword(Keyword::FOR) {
            self.expect_keyword(Keyword::EACH)?;
            match self.peek_keyword() {
                Some(Keyword::ROW) => {
                    self.advance();
                    TriggerForEach::Row
                }
                Some(Keyword::STATEMENT) => {
                    self.advance();
                    TriggerForEach::Statement
                }
                _ => TriggerForEach::Statement,
            }
        } else {
            TriggerForEach::Statement
        };

        let when = if self.try_consume_keyword(Keyword::WHEN) {
            self.expect_token(&Token::LParen).ok();
            let expr = self.parse_expr().ok();
            while !matches!(self.peek(), Token::RParen | Token::Eof) {
                self.advance();
            }
            if self.match_token(&Token::RParen) {
                self.advance();
            }
            expr
        } else {
            None
        };

        self.expect_keyword(Keyword::EXECUTE)?;
        let execute_kind = if self.try_consume_keyword(Keyword::FUNCTION) {
            ExecuteKind::Function
        } else {
            self.expect_keyword(Keyword::PROCEDURE)?;
            ExecuteKind::Procedure
        };
        let func_name = self.parse_object_name()?;

        let mut func_args = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let arg = self.parse_expr()?;
                    func_args.push(arg);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        Ok(CreateTriggerStatement {
            name,
            or_replace,
            constraint,
            timing,
            table,
            events,
            for_each,
            when,
            func_name,
            func_args,
            execute_kind,
        })
    }

    fn skip_balanced_expr(&mut self) -> Result<String, ParserError> {
        let mut s = String::new();
        let mut depth = 0;
        loop {
            match self.peek() {
                Token::Comma if depth == 0 => break,
                Token::RParen if depth == 0 => break,
                Token::Semicolon if depth == 0 => break,
                Token::LParen => {
                    depth += 1;
                    s.push('(');
                    self.advance();
                }
                Token::RParen => {
                    depth -= 1;
                    s.push(')');
                    self.advance();
                }
                Token::Eof => break,
                _ => {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(&self.token_to_string());
                    self.advance();
                }
            }
        }
        Ok(s.trim().to_string())
    }

    pub(crate) fn parse_create_materialized_view(&mut self) -> Result<CreateMaterializedViewStatement, ParserError> {
        self.expect_keyword(Keyword::VIEW)?;

        let if_not_exists = self.try_consume_keyword(Keyword::IF_P)
            && self.try_consume_keyword(Keyword::NOT)
            && self.try_consume_keyword(Keyword::EXISTS);

        let name = self.parse_object_name()?;

        let mut columns = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    columns.push(self.parse_identifier()?);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        self.expect_keyword(Keyword::AS)?;

        let query = Box::new(self.parse_select_statement()?);

        let mut tablespace = None;
        if self.try_consume_keyword(Keyword::TABLESPACE) {
            tablespace = Some(self.parse_identifier()?);
        }

        let mut with_data = true;
        if self.try_consume_keyword(Keyword::WITH) {
            if self.try_consume_keyword(Keyword::NO) {
                self.try_consume_keyword(Keyword::DATA_P);
                with_data = false;
            } else {
                self.try_consume_keyword(Keyword::DATA_P);
                with_data = true;
            }
        }

        Ok(CreateMaterializedViewStatement { if_not_exists, name, columns, query, tablespace, with_data })
    }

    pub(crate) fn parse_refresh_materialized_view(&mut self) -> Result<RefreshMatViewStatement, ParserError> {
        self.expect_keyword(Keyword::MATERIALIZED)?;
        self.expect_keyword(Keyword::VIEW)?;

        let concurrent = self.try_consume_keyword(Keyword::CONCURRENTLY);
        let name = self.parse_object_name()?;

        Ok(RefreshMatViewStatement { concurrent, name })
    }

    // ── Wave 9: VACUUM / ANALYZE / COMMENT ON / LOCK TABLE ──

    pub(crate) fn parse_vacuum(&mut self) -> Result<VacuumStatement, ParserError> {
        let mut full = false;
        let mut verbose = false;
        let mut analyze = false;
        let mut freeze = false;

        // Disambiguate: VACUUM (VERBOSE, ANALYZE) table vs VACUUM table(col)
        if self.match_token(&Token::LParen) {
            let is_option_list = matches!(
                self.peek_keyword_at(1),
                Some(Keyword::FULL) | Some(Keyword::VERBOSE) | Some(Keyword::ANALYZE) | Some(Keyword::FREEZE)
            );
            if is_option_list {
                self.advance();
                loop {
                    match self.peek_keyword() {
                        Some(Keyword::FULL) => {
                            self.advance();
                            full = true;
                        }
                        Some(Keyword::VERBOSE) => {
                            self.advance();
                            verbose = true;
                        }
                        Some(Keyword::ANALYZE) => {
                            self.advance();
                            analyze = true;
                        }
                        Some(Keyword::FREEZE) => {
                            self.advance();
                            freeze = true;
                        }
                        _ => break,
                    }
                    if !self.match_token(&Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect_token(&Token::RParen)?;
            }
        }

        loop {
            match self.peek_keyword() {
                Some(Keyword::FULL) => {
                    self.advance();
                    full = true;
                }
                Some(Keyword::VERBOSE) => {
                    self.advance();
                    verbose = true;
                }
                Some(Keyword::ANALYZE) => {
                    self.advance();
                    analyze = true;
                }
                Some(Keyword::FREEZE) => {
                    self.advance();
                    freeze = true;
                }
                _ => break,
            }
        }

        let mut tables = Vec::new();
        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            let name = self.parse_object_name()?;
            let mut columns = Vec::new();
            if self.match_token(&Token::LParen) {
                self.advance();
                if !self.match_token(&Token::RParen) {
                    loop {
                        columns.push(self.parse_identifier()?);
                        if self.match_token(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect_token(&Token::RParen)?;
            }
            tables.push(VacuumTarget { name, columns });
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }

        Ok(VacuumStatement { full, verbose, analyze, freeze, tables })
    }

    pub(crate) fn parse_analyze(&mut self) -> Result<AnalyzeStatement, ParserError> {
        let mut verbose = false;

        if self.try_consume_keyword(Keyword::VERBOSE) {
            verbose = true;
        }

        let mut tables = Vec::new();
        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            let name = self.parse_object_name()?;
            let mut columns = Vec::new();
            if self.match_token(&Token::LParen) {
                self.advance();
                if !self.match_token(&Token::RParen) {
                    loop {
                        columns.push(self.parse_identifier()?);
                        if self.match_token(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect_token(&Token::RParen)?;
            }
            tables.push(VacuumTarget { name, columns });
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }

        let mut options = Vec::new();
        if self.try_consume_keyword(Keyword::WITH) {
            loop {
                let opt = self.consume_any_identifier()?;
                options.push(opt);
                if !self.match_token(&Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        Ok(AnalyzeStatement { verbose, tables, options })
    }

    pub(crate) fn parse_comment(&mut self) -> Result<CommentStatement, ParserError> {
        self.expect_keyword(Keyword::ON)?;

        let object_type = match self.peek_keyword() {
            Some(Keyword::COLUMN) => {
                self.advance();
                "COLUMN"
            }
            Some(Keyword::TABLE) => {
                self.advance();
                "TABLE"
            }
            Some(Keyword::VIEW) => {
                self.advance();
                "VIEW"
            }
            Some(Keyword::MATERIALIZED) => {
                self.advance();
                self.expect_keyword(Keyword::VIEW)?;
                "MATERIALIZED VIEW"
            }
            Some(Keyword::INDEX) => {
                self.advance();
                "INDEX"
            }
            Some(Keyword::SEQUENCE) => {
                self.advance();
                "SEQUENCE"
            }
            Some(Keyword::DATABASE) => {
                self.advance();
                "DATABASE"
            }
            Some(Keyword::SCHEMA) => {
                self.advance();
                "SCHEMA"
            }
            Some(Keyword::DOMAIN_P) => {
                self.advance();
                "DOMAIN"
            }
            Some(Keyword::TYPE_P) => {
                self.advance();
                "TYPE"
            }
            Some(Keyword::AGGREGATE) => {
                self.advance();
                "AGGREGATE"
            }
            Some(Keyword::FUNCTION) => {
                self.advance();
                "FUNCTION"
            }
            Some(Keyword::TABLESPACE) => {
                self.advance();
                "TABLESPACE"
            }
            Some(Keyword::EXTENSION) => {
                self.advance();
                "EXTENSION"
            }
            Some(Keyword::ROLE) => {
                self.advance();
                "ROLE"
            }
            Some(Keyword::SERVER) => {
                self.advance();
                "SERVER"
            }
            Some(Keyword::COLLATION) => {
                self.advance();
                "COLLATION"
            }
            Some(Keyword::FOREIGN) => {
                self.advance();
                if self.match_keyword(Keyword::TABLE) {
                    self.advance();
                    "FOREIGN TABLE"
                } else {
                    self.expect_keyword(Keyword::DATA_P)?;
                    self.expect_keyword(Keyword::WRAPPER)?;
                    "FOREIGN DATA WRAPPER"
                }
            }
            _ => {
                let ot = self.parse_identifier()?;
                return self.parse_comment_body(ot.to_uppercase());
            }
        };

        self.parse_comment_body(object_type.to_string())
    }

    fn parse_comment_body(&mut self, object_type: String) -> Result<CommentStatement, ParserError> {
        let name = self.parse_object_name()?;
        self.expect_keyword(Keyword::IS)?;
        let comment = self.parse_string_literal()?;
        Ok(CommentStatement { object_type, name, comment })
    }

    pub(crate) fn parse_lock(&mut self) -> Result<LockStatement, ParserError> {
        self.expect_keyword(Keyword::TABLE)?;

        let mut tables = Vec::new();
        tables.push(self.parse_object_name()?);
        while self.match_token(&Token::Comma) {
            self.advance();
            tables.push(self.parse_object_name()?);
        }

        let mut mode = String::new();
        if self.try_consume_keyword(Keyword::IN_P) {
            loop {
                match self.peek() {
                    Token::Keyword(kw) => {
                        if !mode.is_empty() {
                            mode.push(' ');
                        }
                        mode.push_str(&kw.as_str().to_uppercase());
                        self.advance();
                        if self.match_keyword(Keyword::MODE) {
                            self.advance();
                            break;
                        }
                    }
                    Token::Eof => break,
                    Token::Semicolon => break,
                    _ => {
                        if !mode.is_empty() {
                            mode.push(' ');
                        }
                        mode.push_str(&self.token_to_string());
                        self.advance();
                        if self.match_keyword(Keyword::MODE) {
                            self.advance();
                            break;
                        }
                    }
                }
            }
        }

        let nowait = self.try_consume_keyword(Keyword::NOWAIT);

        Ok(LockStatement { tables, mode: mode.trim_end_matches(" MODE").to_string(), nowait })
    }

    // ── Wave 10: PREPARE / EXECUTE / DEALLOCATE / DO ──

    pub(crate) fn parse_prepare(&mut self) -> Result<PrepareStatement, ParserError> {
        let name = self.parse_identifier()?;

        let mut data_types = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let mut dt = self.parse_identifier()?;
                    if self.match_token(&Token::LParen) {
                        self.advance();
                        dt.push('(');
                        let mut first = true;
                        while !self.match_token(&Token::RParen) {
                            if !first && self.match_token(&Token::Comma) {
                                self.advance();
                                dt.push_str(", ");
                            }
                            first = false;
                            let mut depth = 0i32;
                            loop {
                                match self.peek() {
                                    crate::token::Token::LParen => {
                                        depth += 1;
                                        dt.push('(');
                                        self.advance();
                                    }
                                    crate::token::Token::RParen if depth > 0 => {
                                        depth -= 1;
                                        dt.push(')');
                                        self.advance();
                                    }
                                    crate::token::Token::RParen => break,
                                    crate::token::Token::Comma if depth == 0 => break,
                                    other => {
                                        dt.push_str(format!("{:?}", other).trim_matches('"'));
                                        self.advance();
                                    }
                                }
                            }
                        }
                        self.expect_token(&Token::RParen)?;
                        dt.push(')');
                    }
                    data_types.push(dt);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        self.expect_keyword(Keyword::AS)?;

        let (statement, parsed_statement) = {
            let save_pos = self.pos;
            if let Some(stmt) = self.try_parse_dml_statement() {
                let raw = self.tokens_to_raw_string(save_pos, self.pos);
                self.try_consume_semicolon();
                (raw, Some(stmt))
            } else {
                self.pos = save_pos;
                let raw = self.skip_to_semicolon_and_collect();
                (raw, None)
            }
        };

        Ok(PrepareStatement { name, data_types, statement, parsed_statement })
    }

    pub(crate) fn parse_execute(&mut self) -> Result<ExecuteStatement, ParserError> {
        let name = self.parse_identifier()?;

        let mut params = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let p = self.parse_expr()?;
                    params.push(p);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        Ok(ExecuteStatement { name, params })
    }

    pub(crate) fn parse_deallocate(&mut self) -> Result<DeallocateStatement, ParserError> {
        self.try_consume_keyword(Keyword::PREPARE);

        if self.match_keyword(Keyword::ALL) {
            self.advance();
            return Ok(DeallocateStatement { name: None, all: true });
        }

        let name = self.parse_identifier()?;
        Ok(DeallocateStatement { name: Some(name), all: false })
    }

    pub(crate) fn parse_do(&mut self) -> Result<DoStatement, ParserError> {
        let mut language = None;

        if self.try_consume_keyword(Keyword::LANGUAGE) {
            language = Some(self.parse_identifier()?);
        }

        // Try to extract dollar-quoted body and parse as PL/pgSQL
        let (code, block) = if matches!(self.peek(), Token::DollarString { .. }) {
            if let Token::DollarString { body: inner, .. } = self.peek().clone() {
                self.advance();
                let inner_str = inner.clone();
                match Self::parse_pl_block_from_str(&inner_str) {
                    Ok(block) => (inner_str, Some(block)),
                    Err(_) => (inner_str, None),
                }
            } else {
                unreachable!()
            }
        } else {
            let code = self.skip_to_semicolon_and_collect();
            (code, None)
        };

        Ok(DoStatement { language, code, block })
    }

    pub(crate) fn parse_pl_block_from_str(input: &str) -> Result<crate::ast::plpgsql::PlBlock, ParserError> {
        let tokens = crate::token::tokenizer::Tokenizer::new(input).tokenize()?;
        let mut parser = Parser::new(tokens);
        parser.parse_pl_block()
    }

    pub(crate) fn parse_statement_from_str(input: &str) -> Option<Box<crate::ast::Statement>> {
        let tokens = match crate::token::tokenizer::Tokenizer::new(input).tokenize() {
            Ok(t) => t,
            Err(_) => return None,
        };
        let mut parser = Parser::new(tokens);
        match parser.parse_statement() {
            Ok(crate::ast::Statement::Empty) => None,
            Ok(stmt) => Some(Box::new(stmt)),
            Err(_) => None,
        }
    }

    pub(crate) fn is_transaction_begin(&self) -> bool {
        let next = match self.tokens.get(self.pos + 1) {
            Some(tw) => &tw.token,
            None => return true,
        };
        match next {
            Token::Eof => true,
            Token::Semicolon => true,
            Token::Slash => true,
            Token::Keyword(Keyword::WORK) => true,
            Token::Keyword(Keyword::TRANSACTION) => true,
            Token::Keyword(Keyword::ISOLATION) => true,
            Token::Keyword(Keyword::DEFERRABLE) => true,
            Token::Keyword(Keyword::NOT) => true,
            Token::Keyword(Keyword::READ) => self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|t| matches!(t.token, Token::Keyword(Keyword::ONLY) | Token::Keyword(Keyword::WRITE))),
            _ => false,
        }
    }

    pub(crate) fn parse_anonymous_block(&mut self) -> Result<crate::ast::AnonyBlockStatement, ParserError> {
        if matches!(self.peek(), Token::DollarString { .. }) {
            if let Token::DollarString { body: inner, .. } = self.peek().clone() {
                self.advance();
                let block = Self::parse_pl_block_from_str(&inner)?;
                return Ok(crate::ast::AnonyBlockStatement { block });
            }
        }

        let begin_location = self.current_location();
        let block = self.parse_pl_block_body(None, Vec::new(), begin_location)?;
        Ok(crate::ast::AnonyBlockStatement { block })
    }

    // ── Wave 11: ALTER DATABASE/SCHEMA/SEQUENCE/FUNCTION/ROLE/USER/SYSTEM ──

    pub(crate) fn parse_alter_database(&mut self) -> Result<AlterDatabaseStatement, ParserError> {
        self.expect_keyword(Keyword::DATABASE)?;
        // Check if next token is an action keyword (SET/RESET/RENAME/OWNER) —
        // if so, no database name is given (e.g. `ALTER DATABASE SET ilm = on`).
        let name = if matches!(
            self.peek_keyword(),
            Some(Keyword::SET)
                | Some(Keyword::RESET)
                | Some(Keyword::RENAME)
                | Some(Keyword::OWNER)
                | Some(Keyword::WITH)
                | Some(Keyword::ENABLE_P)
        ) {
            String::new()
        } else {
            self.parse_identifier()?
        };
        let action = self.parse_alter_database_action()?;
        Ok(AlterDatabaseStatement { name, action })
    }

    fn parse_alter_database_action(&mut self) -> Result<AlterDatabaseAction, ParserError> {
        match self.peek_keyword() {
            Some(Keyword::SET) => {
                self.advance();
                let parameter = self.parse_identifier()?;
                if self.match_keyword(Keyword::TO) {
                    self.advance();
                } else if self.match_token(&Token::Eq) {
                    self.advance();
                }
                // SET values can be reserved keywords (ON, OFF, etc.)
                let value = match self.peek().clone() {
                    Token::Ident(s) | Token::QuotedIdent(s) => {
                        self.advance();
                        s
                    }
                    Token::Keyword(kw) => {
                        self.advance();
                        kw.as_str().to_string()
                    }
                    _ => self.parse_identifier()?,
                };
                Ok(AlterDatabaseAction::Set { parameter, value })
            }
            Some(Keyword::RESET) => {
                self.advance();
                let parameter = self.parse_identifier()?;
                Ok(AlterDatabaseAction::Reset { parameter })
            }
            Some(Keyword::RENAME) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let new_name = self.parse_identifier()?;
                Ok(AlterDatabaseAction::RenameTo { new_name })
            }
            Some(Keyword::OWNER) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let owner = self.parse_identifier()?;
                Ok(AlterDatabaseAction::OwnerTo { owner })
            }
            Some(Keyword::WITH) => {
                self.advance();
                if !self.try_consume_ident_str("CONNECTION") {
                    self.expect_keyword(Keyword::CONNECTION)?;
                }
                self.expect_keyword(Keyword::LIMIT)?;
                let limit = self.parse_integer_literal()?;
                Ok(AlterDatabaseAction::WithConnectionLimit { limit })
            }
            Some(Keyword::ENABLE_P) => {
                self.advance();
                self.try_consume_ident_str("PRIVATE");
                self.try_consume_ident_str("OBJECT");
                Ok(AlterDatabaseAction::EnablePrivateObject)
            }
            _ => Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "SET | RESET | RENAME TO | OWNER TO".to_string(),
                got: format!("{:?}", self.peek()),
            }),
        }
    }

    pub(crate) fn parse_alter_schema(&mut self) -> Result<AlterSchemaStatement, ParserError> {
        self.expect_keyword(Keyword::SCHEMA)?;
        let name = self.parse_identifier()?;
        let action = self.parse_alter_schema_action()?;
        Ok(AlterSchemaStatement { name, action })
    }

    fn parse_alter_schema_action(&mut self) -> Result<AlterSchemaAction, ParserError> {
        match self.peek_keyword() {
            Some(Keyword::RENAME) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let new_name = self.parse_identifier()?;
                Ok(AlterSchemaAction::RenameTo { new_name })
            }
            Some(Keyword::OWNER) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let owner = self.parse_identifier()?;
                Ok(AlterSchemaAction::OwnerTo { owner })
            }
            Some(Keyword::CHARACTER) => {
                self.advance();
                self.expect_keyword(Keyword::SET)?;
                let charset = self.parse_identifier()?;
                let mut collate = None;
                if self.try_consume_keyword(Keyword::COLLATE) {
                    collate = Some(self.parse_identifier()?);
                }
                Ok(AlterSchemaAction::CharacterSet { charset, collate })
            }
            _ => Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "RENAME TO | OWNER TO".to_string(),
                got: format!("{:?}", self.peek()),
            }),
        }
    }

    pub(crate) fn parse_alter_sequence(&mut self) -> Result<AlterSequenceStatement, ParserError> {
        self.expect_keyword(Keyword::SEQUENCE)?;
        self.parse_alter_sequence_inner()
    }

    pub(crate) fn parse_alter_sequence_inner(&mut self) -> Result<AlterSequenceStatement, ParserError> {
        let name = self.parse_object_name()?;
        let mut options = Vec::new();

        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            match self.peek_keyword() {
                Some(Keyword::INCREMENT) => {
                    self.advance();
                    self.expect_keyword(Keyword::BY)?;
                    let val = self.parse_integer_literal()?;
                    options.push(SequenceOption::IncrementBy(val));
                }
                Some(Keyword::MINVALUE) => {
                    self.advance();
                    if self.match_keyword(Keyword::NO) {
                        self.advance();
                        options.push(SequenceOption::MinValue(None));
                    } else {
                        let val = self.parse_integer_literal()?;
                        options.push(SequenceOption::MinValue(Some(val)));
                    }
                }
                Some(Keyword::MAXVALUE) => {
                    self.advance();
                    if self.match_keyword(Keyword::NO) {
                        self.advance();
                        options.push(SequenceOption::MaxValue(None));
                    } else {
                        let val = self.parse_integer_literal()?;
                        options.push(SequenceOption::MaxValue(Some(val)));
                    }
                }
                Some(Keyword::START) => {
                    self.advance();
                    self.expect_keyword(Keyword::WITH)?;
                    let val = self.parse_integer_literal()?;
                    options.push(SequenceOption::StartWith(val));
                }
                Some(Keyword::RESTART) => {
                    self.advance();
                    if self.match_keyword(Keyword::WITH) {
                        self.advance();
                        let val = self.parse_integer_literal()?;
                        options.push(SequenceOption::Restart(true));
                        options.push(SequenceOption::StartWith(val));
                    } else {
                        options.push(SequenceOption::Restart(true));
                    }
                }
                Some(Keyword::CACHE) => {
                    self.advance();
                    let val = self.parse_integer_literal()?;
                    options.push(SequenceOption::Cache(val));
                }
                Some(Keyword::CYCLE) => {
                    self.advance();
                    options.push(SequenceOption::Cycle(true));
                }
                Some(Keyword::OWNED) => {
                    self.advance();
                    self.expect_keyword(Keyword::BY)?;
                    let owner = self.parse_object_name()?;
                    options.push(SequenceOption::OwnedBy { owner });
                }
                Some(Keyword::OWNER) => {
                    self.advance();
                    self.expect_keyword(Keyword::TO)?;
                    let owner = self.parse_object_name()?;
                    options.push(SequenceOption::OwnerTo { owner });
                }
                Some(Keyword::NO) => {
                    self.advance();
                    match self.peek_keyword() {
                        Some(Keyword::MINVALUE) => {
                            self.advance();
                            options.push(SequenceOption::MinValue(None));
                        }
                        Some(Keyword::MAXVALUE) => {
                            self.advance();
                            options.push(SequenceOption::MaxValue(None));
                        }
                        Some(Keyword::CYCLE) => {
                            self.advance();
                            options.push(SequenceOption::Cycle(false));
                        }
                        _ => break,
                    }
                }
                _ => break,
            }
        }

        Ok(AlterSequenceStatement { name, options, is_large: false })
    }

    pub(crate) fn parse_integer_literal(&mut self) -> Result<i64, ParserError> {
        match self.peek().clone() {
            Token::Integer(i) => {
                self.advance();
                Ok(i)
            }
            _ => Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "integer literal".to_string(),
                got: format!("{:?}", self.peek()),
            }),
        }
    }

    pub(crate) fn parse_signed_integer(&mut self) -> Result<i64, ParserError> {
        let neg = self.match_token(&Token::Minus);
        if neg {
            self.advance();
        }
        let n = self.parse_integer_literal()?;
        Ok(if neg { -n } else { n })
    }

    pub(crate) fn parse_alter_function(&mut self) -> Result<AlterFunctionStatement, ParserError> {
        self.expect_keyword(Keyword::FUNCTION)?;
        self.parse_alter_function_body()
    }

    pub(crate) fn parse_alter_function_skip_keyword(&mut self) -> Result<AlterFunctionStatement, ParserError> {
        self.parse_alter_function_body()
    }

    fn parse_alter_function_body(&mut self) -> Result<AlterFunctionStatement, ParserError> {
        let name = self.parse_object_name()?;

        if self.match_token(&Token::LParen) {
            self.advance();
            let mut depth = 0;
            loop {
                match self.peek() {
                    Token::LParen => {
                        depth += 1;
                        self.advance();
                    }
                    Token::RParen if depth == 0 => {
                        self.advance();
                        break;
                    }
                    Token::RParen => {
                        depth -= 1;
                        self.advance();
                    }
                    _ => self.advance(),
                }
            }
        }

        let action = match self.peek_keyword() {
            Some(Keyword::RENAME) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let new_name = self.parse_identifier()?;
                AlterFunctionAction::RenameTo { new_name }
            }
            Some(Keyword::OWNER) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let owner = self.parse_identifier()?;
                AlterFunctionAction::OwnerTo { owner }
            }
            Some(Keyword::SET) => {
                self.advance();
                let parameter = self.parse_identifier()?;
                self.expect_keyword(Keyword::TO)?;
                let value = self.parse_identifier()?;
                AlterFunctionAction::Set { parameter, value }
            }
            Some(Keyword::RESET) => {
                self.advance();
                let parameter = self.parse_identifier()?;
                AlterFunctionAction::Reset { parameter }
            }
            Some(Keyword::SCHEMA) => {
                self.advance();
                let schema = self.parse_identifier()?;
                AlterFunctionAction::SetSchema { schema }
            }
            Some(Keyword::IMMUTABLE) => {
                self.advance();
                AlterFunctionAction::Immutable
            }
            Some(Keyword::STABLE) => {
                self.advance();
                AlterFunctionAction::Stable
            }
            Some(Keyword::VOLATILE) => {
                self.advance();
                AlterFunctionAction::Volatile
            }
            Some(Keyword::LEAKPROOF) => {
                self.advance();
                AlterFunctionAction::Leakproof { not: false }
            }
            Some(Keyword::NOT) if self.peek_keyword_at(1) == Some(Keyword::LEAKPROOF) => {
                self.advance();
                self.advance();
                AlterFunctionAction::Leakproof { not: true }
            }
            Some(Keyword::STRICT_P) => {
                self.advance();
                AlterFunctionAction::Strict
            }
            Some(Keyword::CALLED) => {
                self.advance();
                self.expect_keyword(Keyword::ON)?;
                self.expect_keyword(Keyword::NULL_P)?;
                self.expect_keyword(Keyword::INPUT_P)?;
                AlterFunctionAction::CalledOnNullInput
            }
            Some(Keyword::RETURNS) => {
                self.advance();
                self.expect_keyword(Keyword::NULL_P)?;
                self.expect_keyword(Keyword::ON)?;
                self.expect_keyword(Keyword::NULL_P)?;
                self.expect_keyword(Keyword::INPUT_P)?;
                AlterFunctionAction::ReturnsNullOnNullInput
            }
            Some(Keyword::SHIPPABLE) => {
                self.advance();
                AlterFunctionAction::Shippable { not: false }
            }
            Some(Keyword::NOT) if self.peek_keyword_at(1) == Some(Keyword::SHIPPABLE) => {
                self.advance();
                self.advance();
                AlterFunctionAction::Shippable { not: true }
            }
            Some(Keyword::NOT) if self.peek_keyword_at(1) == Some(Keyword::STRICT_P) => {
                self.advance();
                self.advance();
                AlterFunctionAction::CalledOnNullInput
            }
            Some(Keyword::PACKAGE) => {
                self.advance();
                AlterFunctionAction::Package { not: false }
            }
            Some(Keyword::NOT) if self.peek_keyword_at(1) == Some(Keyword::PACKAGE) => {
                self.advance();
                self.advance();
                AlterFunctionAction::Package { not: true }
            }
            _ if self.match_ident_str("COMPILE") => {
                self.advance();
                AlterFunctionAction::Compile
            }
            _ => {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "RENAME TO | OWNER TO | SET | RESET | SCHEMA".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
        };

        Ok(AlterFunctionStatement { name, action })
    }

    pub(crate) fn parse_alter_role(&mut self) -> Result<AlterRoleStatement, ParserError> {
        self.expect_keyword(Keyword::ROLE)?;
        let name = self.parse_identifier()?;
        let mut options = Vec::new();

        self.try_consume_keyword(Keyword::WITH);

        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            match self.peek_keyword() {
                Some(Keyword::PASSWORD) => {
                    self.advance();
                    let value = self.parse_string_literal()?;
                    options.push(("PASSWORD".to_string(), Some(value)));
                }
                Some(Keyword::IDENTIFIED) => {
                    self.advance();
                    self.expect_keyword(Keyword::BY)?;
                    let value = self.parse_string_literal()?;
                    options.push(("IDENTIFIED BY".to_string(), Some(value)));
                }
                Some(Keyword::REPLACE) => {
                    self.advance();
                    let value = self.parse_string_literal()?;
                    options.push(("REPLACE".to_string(), Some(value)));
                }
                Some(Keyword::ENCRYPTED) => {
                    self.advance();
                    options.push(("ENCRYPTED".to_string(), None));
                }
                Some(Keyword::UNENCRYPTED) => {
                    self.advance();
                    options.push(("UNENCRYPTED".to_string(), None));
                }
                Some(Keyword::VALID) => {
                    self.advance();
                    self.expect_keyword(Keyword::UNTIL)?;
                    let value = self.parse_string_literal()?;
                    options.push(("VALID UNTIL".to_string(), Some(value)));
                }
                Some(Keyword::RENAME) => {
                    self.advance();
                    self.expect_keyword(Keyword::TO)?;
                    let value = self.parse_identifier()?;
                    options.push(("RENAME TO".to_string(), Some(value)));
                }
                Some(Keyword::INHERIT) => {
                    self.advance();
                    options.push(("INHERIT".to_string(), None));
                }
                _ => {
                    if let Token::Ident(s) = self.peek() {
                        let upper = s.to_uppercase();
                        match upper.as_str() {
                            "SUPERUSER" | "NOSUPERUSER" | "CREATEDB" | "NOCREATEDB" | "CREATEROLE" | "NOCREATEROLE"
                            | "LOGIN" | "NOLOGIN" | "NOINHERIT" => {
                                self.advance();
                                options.push((upper, None));
                                continue;
                            }
                            _ => {
                                let key = self.parse_identifier()?;
                                if self.match_token(&Token::Eq) {
                                    self.advance();
                                    let value = self.parse_identifier()?;
                                    options.push((key, Some(value)));
                                } else {
                                    options.push((key, None));
                                }
                                continue;
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(AlterRoleStatement { name, options })
    }

    pub(crate) fn parse_alter_user(&mut self) -> Result<AlterUserStatement, ParserError> {
        self.expect_keyword(Keyword::USER)?;
        self.parse_alter_user_inner()
    }

    pub(crate) fn parse_alter_user_inner(&mut self) -> Result<AlterUserStatement, ParserError> {
        let name = self.parse_identifier()?;
        let mut options = Vec::new();

        self.try_consume_keyword(Keyword::WITH);

        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            match self.peek_keyword() {
                Some(Keyword::PASSWORD) => {
                    self.advance();
                    let value = self.parse_string_literal()?;
                    options.push(("PASSWORD".to_string(), Some(value)));
                }
                Some(Keyword::ENCRYPTED) => {
                    self.advance();
                    options.push(("ENCRYPTED".to_string(), None));
                }
                Some(Keyword::UNENCRYPTED) => {
                    self.advance();
                    options.push(("UNENCRYPTED".to_string(), None));
                }
                Some(Keyword::RENAME) => {
                    self.advance();
                    self.expect_keyword(Keyword::TO)?;
                    let value = self.parse_identifier()?;
                    options.push(("RENAME TO".to_string(), Some(value)));
                }
                _ => {
                    let key = self.consume_any_identifier()?;
                    if key.to_uppercase() == "IDENTIFIED" {
                        self.expect_keyword(Keyword::BY)?;
                        let password = self.parse_string_literal()?;
                        options.push(("IDENTIFIED BY".to_string(), Some(password)));
                        if self.match_keyword(Keyword::REPLACE) {
                            self.advance();
                            let old_password = self.parse_string_literal()?;
                            options.push(("REPLACE".to_string(), Some(old_password)));
                        }
                    } else if self.match_token(&Token::Eq) {
                        self.advance();
                        let value = self.parse_identifier()?;
                        options.push((key, Some(value)));
                    } else {
                        options.push((key, None));
                    }
                }
            }
        }

        Ok(AlterUserStatement { name, options })
    }

    pub(crate) fn parse_alter_global_config(&mut self) -> Result<AlterGlobalConfigStatement, ParserError> {
        self.expect_keyword(Keyword::SYSTEM_P)?;
        self.expect_keyword(Keyword::SET)?;

        let action = AlterGlobalConfigAction::Set {
            parameter: self.parse_identifier()?,
            value: {
                self.try_consume_keyword(Keyword::TO);
                if self.match_token(&Token::Eq) {
                    self.advance();
                }
                self.parse_identifier_or_value()?
            },
        };

        Ok(AlterGlobalConfigStatement { action })
    }

    fn parse_identifier_or_value(&mut self) -> Result<String, ParserError> {
        match self.peek().clone() {
            Token::Ident(s) => {
                self.advance();
                Ok(s)
            }
            Token::QuotedIdent(s) => {
                self.advance();
                Ok(s)
            }
            Token::Keyword(kw) => {
                self.advance();
                Ok(kw.as_str().to_string())
            }
            Token::Integer(i) => {
                self.advance();
                Ok(i.to_string())
            }
            Token::Float(f) => {
                self.advance();
                Ok(f)
            }
            Token::StringLiteral(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "identifier or value".to_string(),
                got: format!("{:?}", self.peek()),
            }),
        }
    }

    // ── Wave 12: CURSOR / LISTEN / NOTIFY / RULE / CLUSTER / REINDEX ──

    pub(crate) fn parse_declare_cursor(&mut self) -> Result<DeclareCursorStatement, ParserError> {
        let name = self.parse_identifier()?;

        let mut binary = false;
        let mut sensitivity = CursorSensitivity::Sensitive;
        let mut scrollability = CursorScrollability::Default;
        let mut holdability = CursorHoldability::Default;
        let mut returnability = CursorReturnability::Default;
        let mut return_to = CursorReturnTo::Default;

        loop {
            match self.peek_keyword() {
                Some(Keyword::BINARY) => {
                    self.advance();
                    binary = true;
                }
                Some(Keyword::INSENSITIVE) => {
                    self.advance();
                    sensitivity = CursorSensitivity::Insensitive;
                }
                Some(Keyword::ASENSITIVE) => {
                    self.advance();
                    sensitivity = CursorSensitivity::Asensitive;
                }
                Some(Keyword::SCROLL) => {
                    self.advance();
                    scrollability = CursorScrollability::Scroll;
                }
                Some(Keyword::NO) => {
                    self.advance();
                    self.try_consume_keyword(Keyword::SCROLL);
                    scrollability = CursorScrollability::NoScroll;
                }
                Some(Keyword::WITH) => {
                    self.advance();
                    if self.match_keyword(Keyword::HOLD) {
                        self.advance();
                        holdability = CursorHoldability::WithHold;
                    } else if self.match_keyword(Keyword::RETURN) {
                        self.advance();
                        returnability = CursorReturnability::WithReturn;
                        if self.match_keyword(Keyword::TO) {
                            self.advance();
                            return_to = self.parse_cursor_return_to()?;
                        }
                    } else {
                        break;
                    }
                }
                Some(Keyword::WITHOUT) => {
                    self.advance();
                    if self.match_keyword(Keyword::HOLD) {
                        self.advance();
                        holdability = CursorHoldability::WithoutHold;
                    } else if self.match_keyword(Keyword::RETURN) {
                        self.advance();
                        returnability = CursorReturnability::WithoutReturn;
                        if self.match_keyword(Keyword::TO) {
                            self.advance();
                            return_to = self.parse_cursor_return_to()?;
                        }
                    } else {
                        break;
                    }
                }
                Some(Keyword::CURSOR) => {
                    self.advance();
                }
                Some(Keyword::FOR) | Some(Keyword::IS) => {
                    break;
                }
                _ => break,
            }
        }

        if !self.try_consume_keyword(Keyword::FOR) && !self.try_consume_keyword(Keyword::IS) {
            self.expect_keyword(Keyword::FOR)?;
        }

        let query = if self.match_keyword(Keyword::VALUES) {
            self.advance();
            let values_stmt = self.parse_values_statement()?;
            Box::new(SelectStatement {
                hints: vec![],
                with: None,
                distinct: false,
                distinct_on: vec![],
                targets: vec![],
                into_targets: None,
                bulk_collect: false,
                into_table: None,
                from: vec![TableRef::Values {
                    values: Box::new(values_stmt),
                    alias: None,
                    column_names: vec![],
                    lateral: false,
                }],
                where_clause: None,
                connect_by: None,
                group_by: vec![],
                having: None,
                order_by: vec![],
                order_siblings: false,
                limit: None,
                offset: None,
                fetch: None,
                lock_clause: None,
                window_clause: vec![],
                set_operation: None,
                raw_body: None,
            })
        } else {
            Box::new(self.parse_select_statement()?)
        };

        Ok(DeclareCursorStatement {
            name,
            binary,
            sensitivity,
            scrollability,
            holdability,
            returnability,
            return_to,
            query,
        })
    }

    fn parse_cursor_return_to(&mut self) -> Result<CursorReturnTo, ParserError> {
        if self.match_ident_str("caller") {
            self.advance();
            Ok(CursorReturnTo::ToCaller)
        } else if self.match_ident_str("client") {
            self.advance();
            Ok(CursorReturnTo::ToClient)
        } else {
            Ok(CursorReturnTo::Default)
        }
    }

    pub(crate) fn parse_fetch_cursor(&mut self) -> Result<FetchStatement, ParserError> {
        let direction = self.parse_cursor_direction()?;

        if self.match_keyword(Keyword::FROM) || self.match_keyword(Keyword::IN_P) {
            self.advance();
        }

        let cursor_name = self.parse_identifier()?;

        Ok(FetchStatement { direction, cursor_name })
    }

    pub(crate) fn parse_move_cursor(&mut self) -> Result<MoveStatement, ParserError> {
        let direction = self.parse_cursor_direction()?;

        if self.match_keyword(Keyword::FROM) || self.match_keyword(Keyword::IN_P) {
            self.advance();
        }

        let cursor_name = self.parse_identifier()?;

        Ok(MoveStatement { direction, cursor_name })
    }

    fn parse_cursor_direction(&mut self) -> Result<FetchDirection, ParserError> {
        match self.peek_keyword() {
            Some(Keyword::NEXT) => {
                self.advance();
                Ok(FetchDirection::Next)
            }
            Some(Keyword::PRIOR) => {
                self.advance();
                Ok(FetchDirection::Prior)
            }
            Some(Keyword::FIRST_P) => {
                self.advance();
                Ok(FetchDirection::First)
            }
            Some(Keyword::LAST_P) => {
                self.advance();
                Ok(FetchDirection::Last)
            }
            Some(Keyword::ABSOLUTE_P) => {
                self.advance();
                let n = self.parse_signed_integer()?;
                Ok(FetchDirection::Absolute(n))
            }
            Some(Keyword::RELATIVE_P) => {
                self.advance();
                let n = self.parse_signed_integer()?;
                Ok(FetchDirection::Relative(n))
            }
            Some(Keyword::FORWARD) => {
                self.advance();
                if self.match_keyword(Keyword::ALL) {
                    self.advance();
                    Ok(FetchDirection::ForwardAll)
                } else if let Token::Integer(n) = self.peek().clone() {
                    self.advance();
                    Ok(FetchDirection::ForwardCount(n))
                } else {
                    Ok(FetchDirection::Forward)
                }
            }
            Some(Keyword::BACKWARD) => {
                self.advance();
                if self.match_keyword(Keyword::ALL) {
                    self.advance();
                    Ok(FetchDirection::BackwardAll)
                } else if let Token::Integer(n) = self.peek().clone() {
                    self.advance();
                    Ok(FetchDirection::BackwardCount(n))
                } else {
                    Ok(FetchDirection::Backward)
                }
            }
            Some(Keyword::ALL) => {
                self.advance();
                Ok(FetchDirection::All)
            }
            _ => {
                if let Token::Integer(n) = self.peek().clone() {
                    self.advance();
                    Ok(FetchDirection::Count(n))
                } else {
                    Ok(FetchDirection::Next)
                }
            }
        }
    }

    pub(crate) fn parse_close_portal(&mut self) -> Result<ClosePortalStatement, ParserError> {
        if self.match_keyword(Keyword::ALL) {
            self.advance();
            Ok(ClosePortalStatement { target: CloseTarget::All })
        } else {
            let name = self.parse_identifier()?;
            Ok(ClosePortalStatement { target: CloseTarget::Name(name) })
        }
    }

    pub(crate) fn parse_listen(&mut self) -> Result<ListenStatement, ParserError> {
        let channel = self.parse_identifier()?;
        Ok(ListenStatement { channel })
    }

    pub(crate) fn parse_notify(&mut self) -> Result<NotifyStatement, ParserError> {
        let channel = self.parse_identifier()?;
        let mut payload = None;
        if self.match_token(&Token::Comma) {
            self.advance();
            payload = Some(self.parse_string_literal()?);
        }
        Ok(NotifyStatement { channel, payload })
    }

    pub(crate) fn parse_unlisten(&mut self) -> Result<UnlistenStatement, ParserError> {
        if self.match_token(&Token::Semicolon) || self.match_token(&Token::Eof) {
            return Ok(UnlistenStatement { channel: None });
        }
        let channel = self.parse_identifier()?;
        Ok(UnlistenStatement { channel: Some(channel) })
    }

    pub(crate) fn parse_rule(&mut self) -> Result<RuleStatement, ParserError> {
        let name = self.parse_identifier()?;
        self.expect_keyword(Keyword::AS)?;
        self.expect_keyword(Keyword::ON)?;

        let event = if self.try_consume_keyword(Keyword::SELECT) {
            RuleEvent::Select
        } else if self.try_consume_keyword(Keyword::INSERT) {
            RuleEvent::Insert
        } else if self.try_consume_keyword(Keyword::UPDATE) {
            RuleEvent::Update
        } else if self.try_consume_keyword(Keyword::DELETE_P) {
            RuleEvent::Delete
        } else {
            let loc = self.current_location();
            return Err(ParserError::UnexpectedToken {
                location: loc,
                expected: "SELECT, INSERT, UPDATE, or DELETE".to_string(),
                got: self.token_to_string().into_owned(),
            });
        };

        self.expect_keyword(Keyword::TO)?;
        let table = self.parse_object_name()?;

        let mut condition = None;
        if self.try_consume_keyword(Keyword::WHERE) {
            condition = Some(self.parse_expr()?);
        }

        let mut instead = false;
        if self.try_consume_keyword(Keyword::DO) && self.try_consume_keyword(Keyword::INSTEAD) {
            instead = true;
        }

        let mut actions = Vec::new();
        if self.try_consume_keyword(Keyword::NOTHING) {
            actions.push("NOTHING".to_string());
        } else if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let action = self.skip_to_semicolon_and_collect();
                    if !action.is_empty() {
                        actions.push(action);
                    }
                    if self.match_token(&Token::Semicolon) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        Ok(RuleStatement { name, table, event, condition, instead, actions, parsed_actions: None })
    }

    pub(crate) fn parse_cluster(&mut self) -> Result<ClusterStatement, ParserError> {
        let mut verbose = false;
        if self.try_consume_keyword(Keyword::VERBOSE) {
            verbose = true;
        }

        let table = if !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            Some(self.parse_object_name()?)
        } else {
            None
        };

        let mut using_index = None;
        if self.try_consume_keyword(Keyword::USING) {
            using_index = Some(self.parse_identifier()?);
        }

        let mut partition = None;
        if self.try_consume_keyword(Keyword::PARTITION) {
            self.expect_token(&Token::LParen)?;
            partition = Some(self.parse_identifier()?);
            self.expect_token(&Token::RParen)?;
            if self.try_consume_keyword(Keyword::USING) {
                using_index = Some(self.parse_identifier()?);
            }
        }

        Ok(ClusterStatement { table, verbose, using_index, partition })
    }

    pub(crate) fn parse_reindex(&mut self) -> Result<ReindexStatement, ParserError> {
        let mut verbose = false;
        let mut concurrent = false;

        if self.try_consume_keyword(Keyword::VERBOSE) {
            verbose = true;
        }

        let target = match self.peek_keyword() {
            Some(Keyword::TABLE) => {
                self.advance();
                if self.try_consume_keyword(Keyword::CONCURRENTLY) {
                    concurrent = true;
                }
                let name = self.parse_object_name()?;
                ReindexTarget::Table(name)
            }
            Some(Keyword::INDEX) => {
                self.advance();
                if self.try_consume_keyword(Keyword::CONCURRENTLY) {
                    concurrent = true;
                }
                let name = self.parse_object_name()?;
                ReindexTarget::Index(name)
            }
            Some(Keyword::SCHEMA) => {
                self.advance();
                ReindexTarget::Schema(self.parse_identifier()?)
            }
            Some(Keyword::DATABASE) => {
                self.advance();
                ReindexTarget::Database(self.parse_identifier()?)
            }
            Some(Keyword::SYSTEM_P) => {
                self.advance();
                ReindexTarget::System
            }
            _ => {
                if self.try_consume_keyword(Keyword::CONCURRENTLY) {
                    concurrent = true;
                }
                ReindexTarget::Index(self.parse_object_name()?)
            }
        };

        Ok(ReindexStatement { target, verbose, concurrent })
    }

    // ── ALTER GROUP ──

    pub(crate) fn parse_alter_group(&mut self) -> Result<AlterGroupStatement, ParserError> {
        self.expect_keyword(Keyword::GROUP_P)?;
        let name = self.parse_identifier()?;
        let action = if self.match_keyword(Keyword::ADD_P) {
            self.advance();
            self.expect_keyword(Keyword::USER)?;
            let user = self.parse_identifier()?;
            while self.match_token(&Token::Comma) {
                self.advance();
                let _ = self.parse_identifier();
            }
            AlterGroupAction::AddUser(user)
        } else if self.match_keyword(Keyword::DROP) {
            self.advance();
            self.expect_keyword(Keyword::USER)?;
            let user = self.parse_identifier()?;
            while self.match_token(&Token::Comma) {
                self.advance();
                let _ = self.parse_identifier();
            }
            AlterGroupAction::DropUser(user)
        } else if self.match_keyword(Keyword::RENAME) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let new_name = self.parse_identifier()?;
            AlterGroupAction::RenameTo(new_name)
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "ADD USER or DROP USER".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterGroupStatement { name, action })
    }

    pub(crate) fn parse_create_aggregate(&mut self) -> Result<CreateAggregateStatement, ParserError> {
        let name = self.parse_object_name()?.join(".");
        let base_types = if self.match_token(&Token::LParen) {
            self.advance();
            if self.match_token(&Token::RParen) {
                self.advance();
                Vec::new()
            } else {
                let mut types = vec![self.parse_data_type()?];
                while self.match_token(&Token::Comma) {
                    self.advance();
                    types.push(self.parse_data_type()?);
                }
                self.expect_token(&Token::RParen)?;
                types
            }
        } else {
            Vec::new()
        };
        let options = self.parse_generic_options_no_with();
        Ok(CreateAggregateStatement { name, base_types, options })
    }

    pub(crate) fn parse_create_operator(&mut self) -> Result<CreateOperatorStatement, ParserError> {
        self.expect_keyword(Keyword::OPERATOR)?;
        let name = match self.peek().clone() {
            Token::Ident(s) => {
                self.advance();
                s
            }
            Token::Op(s) => {
                self.advance();
                s
            }
            tok @ (Token::OpLe
            | Token::OpNe
            | Token::OpGe
            | Token::OpShiftL
            | Token::OpShiftR
            | Token::OpArrow
            | Token::OpJsonArrow
            | Token::OpNe2
            | Token::OpDblBang
            | Token::OpConcat) => {
                self.advance();
                tok.as_op_str().expect("token matched as operator variant").to_string()
            }
            other => {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "operator name".to_string(),
                    got: format!("{:?}", other),
                });
            }
        };
        let options = self.parse_generic_options_no_with();
        Ok(CreateOperatorStatement { name, options })
    }

    pub(crate) fn parse_alter_default_privileges(&mut self) -> Result<AlterDefaultPrivilegesStatement, ParserError> {
        self.expect_keyword(Keyword::PRIVILEGES)?;
        let mut role = None;
        let mut schema = None;
        if self.try_consume_keyword(Keyword::FOR) {
            self.try_consume_keyword(Keyword::ROLE);
            role = Some(self.parse_identifier()?);
        }
        if self.try_consume_keyword(Keyword::IN_P) {
            self.try_consume_keyword(Keyword::SCHEMA);
            schema = Some(self.parse_identifier()?);
        }
        let action = if self.match_keyword(Keyword::GRANT) {
            self.advance();
            DefaultPrivilegeAction::Grant(self.parse_grant()?)
        } else if self.match_keyword(Keyword::REVOKE) {
            self.advance();
            DefaultPrivilegeAction::Revoke(self.parse_revoke()?)
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "GRANT or REVOKE".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterDefaultPrivilegesStatement { role, schema, action })
    }

    pub(crate) fn parse_create_user_mapping(&mut self) -> Result<CreateUserMappingStatement, ParserError> {
        let if_not_exists = self.parse_if_not_exists();
        self.expect_keyword(Keyword::FOR)?;
        let user_name = self.parse_identifier()?;
        self.expect_keyword(Keyword::SERVER)?;
        let server = self.parse_object_name()?;
        let options = self.parse_options_clause();
        Ok(CreateUserMappingStatement { if_not_exists, user_name, server, options })
    }

    pub(crate) fn parse_alter_user_mapping(&mut self) -> Result<AlterUserMappingStatement, ParserError> {
        self.expect_keyword(Keyword::MAPPING)?;
        self.expect_keyword(Keyword::FOR)?;
        let user_name = self.parse_identifier()?;
        self.expect_keyword(Keyword::SERVER)?;
        let server = self.parse_object_name()?;
        let options = self.parse_options_clause();
        Ok(AlterUserMappingStatement { user_name, server, options })
    }

    pub(crate) fn parse_drop_user_mapping(&mut self) -> Result<DropUserMappingStatement, ParserError> {
        self.expect_keyword(Keyword::USER)?;
        self.expect_keyword(Keyword::MAPPING)?;
        let if_exists = self.parse_if_exists();
        self.expect_keyword(Keyword::FOR)?;
        let user_name = self.parse_identifier()?;
        self.expect_keyword(Keyword::SERVER)?;
        let server = self.parse_object_name()?;
        Ok(DropUserMappingStatement { if_exists, user_name, server })
    }

    pub(crate) fn parse_shutdown(&mut self) -> Result<ShutdownStatement, ParserError> {
        let mode = match self.peek_keyword() {
            Some(Keyword::FAST) => {
                self.advance();
                Some("FAST".to_string())
            }
            Some(Keyword::IMMEDIATE) => {
                self.advance();
                Some("IMMEDIATE".to_string())
            }
            _ => None,
        };
        Ok(ShutdownStatement { mode })
    }

    pub(crate) fn parse_barrier(&mut self) -> Result<BarrierStatement, ParserError> {
        let name = self.parse_identifier()?;
        Ok(BarrierStatement { name })
    }

    pub(crate) fn parse_purge(&mut self) -> Result<PurgeStatement, ParserError> {
        let target = match self.peek_keyword() {
            Some(Keyword::TABLE) => {
                self.advance();
                let name = self.parse_object_name()?;
                PurgeTarget::Table { name }
            }
            Some(Keyword::INDEX) => {
                self.advance();
                let name = self.parse_object_name()?;
                PurgeTarget::Index { name }
            }
            Some(Keyword::SNAPSHOT) => {
                self.advance();
                let name = self.parse_snapshot_qualified_name()?;
                PurgeTarget::Snapshot { name }
            }
            _ => {
                let id = self.parse_identifier()?;
                if id.to_uppercase() == "RECYCLEBIN" {
                    PurgeTarget::RecycleBin
                } else {
                    PurgeTarget::RecycleBin
                }
            }
        };
        Ok(PurgeStatement { target })
    }

    pub(crate) fn parse_snapshot(&mut self) -> Result<SnapshotStatement, ParserError> {
        let name = if self.match_token(&Token::Semicolon) || self.match_token(&Token::Eof) {
            None
        } else {
            Some(self.parse_identifier()?)
        };
        let mut options = Vec::new();
        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            let key = self.parse_identifier().unwrap_or_default();
            let value = if self.match_token(&Token::Eq) {
                self.advance();
                self.parse_identifier().unwrap_or_default()
            } else {
                String::new()
            };
            options.push((key, value));
        }
        Ok(SnapshotStatement { name, options })
    }

    pub(crate) fn parse_timecapsule(&mut self) -> Result<TimeCapsuleStatement, ParserError> {
        self.expect_keyword(Keyword::TABLE)?;
        let table_name = self.parse_object_name()?;
        let action = self.skip_to_semicolon_and_collect();
        Ok(TimeCapsuleStatement { table_name, action: action.clone(), raw_rest: action })
    }

    pub(crate) fn parse_shrink(&mut self) -> Result<ShrinkStatement, ParserError> {
        let target = if self.match_token(&Token::Semicolon) || self.match_token(&Token::Eof) {
            None
        } else {
            Some(self.parse_identifier()?)
        };
        let raw_rest = self.skip_to_semicolon_and_collect();
        Ok(ShrinkStatement { target, raw_rest })
    }

    pub(crate) fn parse_verify(&mut self) -> Result<VerifyStatement, ParserError> {
        let raw_rest = self.skip_to_semicolon_and_collect();
        Ok(VerifyStatement { raw_rest })
    }

    pub(crate) fn parse_compile(&mut self) -> Result<CompileStatement, ParserError> {
        let raw_rest = self.skip_to_semicolon_and_collect();
        Ok(CompileStatement { raw_rest })
    }

    pub(crate) fn parse_clean_conn(&mut self) -> Result<CleanConnStatement, ParserError> {
        self.expect_keyword(Keyword::CONNECTION)?;
        self.expect_keyword(Keyword::TO)?;
        self.expect_keyword(Keyword::ALL)?;

        let force = self.try_consume_keyword(Keyword::FORCE);

        let mut for_database = None;
        let mut to_user = None;

        while !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            if self.match_keyword(Keyword::FOR) {
                self.advance();
                if self.try_consume_keyword(Keyword::DATABASE) {
                    for_database = Some(self.parse_identifier()?);
                } else if self.try_consume_keyword(Keyword::USER) {
                    to_user = Some(self.parse_identifier()?);
                } else {
                    return Err(ParserError::UnexpectedToken {
                        location: self.current_location(),
                        expected: "DATABASE or USER".to_string(),
                        got: format!("{:?}", self.peek()),
                    });
                }
            } else if self.match_keyword(Keyword::TO) {
                self.advance();
                self.expect_keyword(Keyword::USER)?;
                to_user = Some(self.parse_identifier()?);
            } else {
                break;
            }
        }

        Ok(CleanConnStatement { force, for_database, to_user })
    }

    pub(crate) fn parse_sec_label(&mut self) -> Result<SecLabelStatement, ParserError> {
        self.expect_keyword(Keyword::LABEL)?;
        self.try_consume_keyword(Keyword::ON);

        let object_type = if self.match_keyword(Keyword::ROLE) {
            self.advance();
            "role".to_string()
        } else if self.match_keyword(Keyword::TABLE) {
            self.advance();
            "table".to_string()
        } else if self.match_keyword(Keyword::COLUMN) {
            self.advance();
            "column".to_string()
        } else if self.match_keyword(Keyword::FUNCTION) {
            self.advance();
            "function".to_string()
        } else if self.match_keyword(Keyword::DATABASE) {
            self.advance();
            "database".to_string()
        } else if self.match_keyword(Keyword::SCHEMA) {
            self.advance();
            "schema".to_string()
        } else if self.match_keyword(Keyword::SEQUENCE) {
            self.advance();
            "sequence".to_string()
        } else if self.match_keyword(Keyword::VIEW) {
            self.advance();
            "view".to_string()
        } else if self.match_keyword(Keyword::USER) {
            self.advance();
            "user".to_string()
        } else if self.match_keyword(Keyword::MATERIALIZED) {
            self.advance();
            "materialized view".to_string()
        } else {
            self.parse_identifier()?
        };

        let name = self.parse_object_name()?;

        let mut provider = None;
        if self.try_consume_keyword(Keyword::FOR) {
            provider = Some(self.parse_identifier()?);
        }

        let mut label = None;
        self.expect_keyword(Keyword::IS)?;
        if self.try_consume_keyword(Keyword::NULL_P) {
        } else if !self.match_token(&Token::Semicolon) && !self.match_token(&Token::Eof) {
            label = Some(self.parse_string_literal()?);
        }

        Ok(SecLabelStatement { object_type, name, provider, label })
    }

    // ── ALTER DATABASE LINK / DIRECTORY / LANGUAGE / LARGE OBJECT / PACKAGE / SESSION / SYSTEM KILL SESSION ──

    pub(crate) fn parse_alter_database_link(&mut self) -> Result<AlterDatabaseLinkStatement, ParserError> {
        let name = self.parse_identifier()?;
        let action = if self.match_ident_str("connect") {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let user = self.parse_identifier()?;
            if !self.try_consume_ident_str("identified") {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "IDENTIFIED".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
            self.expect_keyword(Keyword::BY)?;
            let password = self.parse_identifier()?;
            let connect_string = if self.match_keyword(Keyword::USING) {
                self.advance();
                Some(self.parse_string_literal()?)
            } else {
                None
            };
            AlterDatabaseLinkAction::ConnectTo { user, password, connect_string }
        } else if self.match_keyword(Keyword::RENAME) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let new_name = self.parse_identifier()?;
            AlterDatabaseLinkAction::RenameTo { new_name }
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "CONNECT TO or RENAME TO".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterDatabaseLinkStatement { name, action })
    }

    pub(crate) fn parse_alter_directory(&mut self) -> Result<AlterDirectoryStatement, ParserError> {
        let name = self.parse_identifier()?;
        let raw_rest = self.skip_to_semicolon_and_collect();
        Ok(AlterDirectoryStatement { name, raw_rest })
    }

    pub(crate) fn parse_alter_language(&mut self) -> Result<AlterLanguageStatement, ParserError> {
        self.try_consume_keyword(Keyword::PROCEDURAL);
        self.expect_keyword(Keyword::LANGUAGE)?;
        let name = self.parse_identifier()?;
        let action = if self.match_keyword(Keyword::RENAME) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let new_name = self.parse_identifier()?;
            AlterLanguageAction::RenameTo(new_name)
        } else if self.match_keyword(Keyword::OWNER) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let owner = self.parse_identifier()?;
            AlterLanguageAction::OwnerTo(owner)
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "RENAME TO or OWNER TO".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterLanguageStatement { name, action })
    }

    pub(crate) fn parse_alter_large_object(&mut self) -> Result<AlterLargeObjectStatement, ParserError> {
        self.expect_keyword(Keyword::OBJECT_P)?;
        let oid = self.parse_identifier()?;
        self.expect_keyword(Keyword::OWNER)?;
        self.expect_keyword(Keyword::TO)?;
        let new_owner = self.parse_identifier()?;
        Ok(AlterLargeObjectStatement { oid, new_owner })
    }

    pub(crate) fn parse_alter_package(&mut self) -> Result<AlterPackageStatement, ParserError> {
        let name = self.parse_identifier()?;
        let action = if self.match_keyword(Keyword::COMPILE) {
            self.advance();
            let debug = self.try_consume_ident_str("debug");
            let reuse_settings = if self.match_keyword(Keyword::REUSE) {
                self.advance();
                if !self.try_consume_ident_str("settings") {
                    return Err(ParserError::UnexpectedToken {
                        location: self.current_location(),
                        expected: "SETTINGS".to_string(),
                        got: format!("{:?}", self.peek()),
                    });
                }
                true
            } else {
                false
            };
            AlterPackageAction::Compile { debug, reuse_settings }
        } else if self.match_keyword(Keyword::OWNER) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let owner = self.parse_identifier()?;
            AlterPackageAction::OwnerTo(owner)
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "COMPILE or OWNER TO".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterPackageStatement { name, action })
    }

    pub(crate) fn parse_alter_session(&mut self) -> Result<AlterSessionStatement, ParserError> {
        let action = if self.match_keyword(Keyword::SET) {
            self.advance();
            let parameter = self.parse_identifier()?;
            if self.match_token(&Token::Eq) {
                self.advance();
            } else {
                self.try_consume_keyword(Keyword::TO);
            }
            let value = self.skip_to_semicolon_and_collect();
            AlterSessionAction::Set { parameter, value }
        } else if self.match_keyword(Keyword::CLOSE) {
            self.advance();
            self.expect_keyword(Keyword::DATABASE)?;
            if !self.try_consume_ident_str("link") {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "LINK".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
            let name = self.parse_identifier()?;
            AlterSessionAction::CloseDatabaseLink { name }
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "SET or CLOSE DATABASE LINK".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterSessionStatement { action })
    }

    pub(crate) fn parse_alter_system_kill_session(&mut self) -> Result<AlterSystemKillSessionStatement, ParserError> {
        let session_id = self.parse_string_literal()?;
        let immediate = self.try_consume_keyword(Keyword::IMMEDIATE);
        Ok(AlterSystemKillSessionStatement { session_id, immediate })
    }

    pub(crate) fn parse_create_language(&mut self) -> Result<CreateLanguageStatement, ParserError> {
        let trusted = self.try_consume_keyword(Keyword::TRUSTED);
        let name = self.parse_identifier()?;
        let mut handler = None;
        let mut inline_func = None;
        let mut validator = None;

        if self.match_keyword(Keyword::HANDLER) {
            self.advance();
            handler = Some(self.parse_identifier()?);
        }
        if self.match_keyword(Keyword::INLINE_P) {
            self.advance();
            inline_func = Some(self.parse_identifier()?);
        }
        if self.match_keyword(Keyword::VALIDATOR) {
            self.advance();
            validator = Some(self.parse_identifier()?);
        } else if self.match_keyword(Keyword::NO) {
            self.advance();
            self.expect_keyword(Keyword::VALIDATOR)?;
        }

        Ok(CreateLanguageStatement { name, trusted, handler, inline_func, validator })
    }

    pub(crate) fn parse_alter_domain(&mut self) -> Result<AlterDomainStatement, ParserError> {
        self.expect_keyword(Keyword::DOMAIN_P)?;
        let name = self.parse_object_name()?;
        let action = if self.match_keyword(Keyword::SET) {
            self.advance();
            if self.match_keyword(Keyword::DEFAULT) {
                self.advance();
                let expr = self.skip_to_semicolon_and_collect();
                AlterDomainAction::SetDefault { expr }
            } else if self.match_keyword(Keyword::NOT) {
                self.advance();
                self.expect_keyword(Keyword::NULL_P)?;
                AlterDomainAction::SetNotNull
            } else {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "DEFAULT | NOT NULL".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
        } else if self.match_keyword(Keyword::DROP) {
            self.advance();
            if self.match_keyword(Keyword::DEFAULT) {
                self.advance();
                AlterDomainAction::DropDefault
            } else if self.match_keyword(Keyword::NOT) {
                self.advance();
                self.expect_keyword(Keyword::NULL_P)?;
                AlterDomainAction::DropNotNull
            } else if self.match_keyword(Keyword::CONSTRAINT) {
                self.advance();
                let cname = self.parse_identifier()?;
                let cascade = self.try_consume_keyword(Keyword::CASCADE);
                self.try_consume_keyword(Keyword::RESTRICT);
                AlterDomainAction::DropConstraint { name: cname, cascade }
            } else {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "DEFAULT | NOT NULL | CONSTRAINT".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
        } else if self.match_keyword(Keyword::ADD_P) {
            self.advance();
            self.try_consume_keyword(Keyword::CONSTRAINT);
            let cname = if !self.match_keyword(Keyword::CHECK) && !self.match_keyword(Keyword::NOT) {
                Some(self.parse_identifier()?)
            } else {
                None
            };
            self.try_consume_keyword(Keyword::CHECK);
            let check_expr = self.skip_to_semicolon_and_collect();
            AlterDomainAction::AddConstraint { name: cname, check_expr }
        } else if self.match_keyword(Keyword::OWNER) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let new_owner = self.parse_identifier()?;
            AlterDomainAction::OwnerTo { new_owner }
        } else if self.match_keyword(Keyword::RENAME) {
            self.advance();
            self.expect_keyword(Keyword::TO)?;
            let new_name = self.parse_identifier()?;
            AlterDomainAction::RenameTo { new_name }
        } else if self.match_keyword(Keyword::VALIDATE) {
            self.advance();
            self.expect_keyword(Keyword::CONSTRAINT)?;
            let cname = self.parse_identifier()?;
            AlterDomainAction::ValidateConstraint { name: cname }
        } else {
            return Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "SET | DROP | ADD | OWNER | RENAME | VALIDATE".to_string(),
                got: format!("{:?}", self.peek()),
            });
        };
        Ok(AlterDomainStatement { name, action })
    }

    fn parse_snapshot_qualified_name(&mut self) -> Result<String, ParserError> {
        let mut name = self.parse_identifier()?;
        if self.match_token(&Token::At) {
            self.advance();
            let version = match &self.tokens.get(self.pos).map(|t| t.token.clone()).unwrap_or(Token::Eof) {
                Token::Float(f) => {
                    self.advance();
                    f.clone()
                }
                Token::Integer(i) => {
                    self.advance();
                    i.to_string()
                }
                Token::Ident(s) => {
                    self.advance();
                    s.clone()
                }
                _ => self.parse_identifier()?,
            };
            name = format!("{}@{}", name, version);
        }
        Ok(name)
    }
}
