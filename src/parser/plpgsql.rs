use crate::ast::plpgsql::{FetchDirection, GetDiagItemKind, *};
use crate::ast::{Expr, Literal, SourceSpan, Spanned};
use crate::parser::{Parser, ParserError};
use crate::token::keyword::Keyword;
use crate::token::Token;

impl Parser {
    pub(crate) fn parse_pl_block(&mut self) -> Result<PlBlock, ParserError> {
        let label = self.try_parse_pl_label();

        let declarations = if self.peek_keyword() == Some(Keyword::DECLARE) {
            self.advance();
            self.parse_pl_declarations()?
        } else {
            Vec::new()
        };

        let begin_location = self.current_location();
        self.expect_keyword(Keyword::BEGIN_P)?;

        self.parse_pl_block_body(label, declarations, begin_location)
    }

    fn declaration_name(&self, decl: &PlDeclaration) -> Option<String> {
        match decl {
            PlDeclaration::Variable(var) => Some(var.name.clone()),
            PlDeclaration::Cursor(cursor) => Some(cursor.name.clone()),
            PlDeclaration::Record(record) => Some(record.name.clone()),
            PlDeclaration::NestedProcedure(_) => None,
            PlDeclaration::NestedFunction(_) => None,
            PlDeclaration::Type(_) => None,
            PlDeclaration::Pragma { .. } => None,
        }
    }

    pub(crate) fn parse_pl_block_body(
        &mut self,
        label: Option<String>,
        declarations: Vec<PlDeclaration>,
        begin_location: crate::token::SourceLocation,
    ) -> Result<PlBlock, ParserError> {
        self.enter_scope()?;
        self.push_scope();
        for decl in &declarations {
            if let Some(name) = self.declaration_name(decl) {
                self.declare_var(&name);
            }
        }
        let result = self.parse_pl_block_body_inner(label, declarations, begin_location);
        self.pop_scope();
        self.leave_scope();
        result
    }

    fn parse_pl_block_body_inner(
        &mut self,
        label: Option<String>,
        declarations: Vec<PlDeclaration>,
        begin_location: crate::token::SourceLocation,
    ) -> Result<PlBlock, ParserError> {
        let mut body = Vec::new();
        let mut exception_block = None;

        loop {
            if matches!(self.peek(), Token::Eof) {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: format!("END (to close BEGIN at line {})", begin_location.line),
                    got: "EOF".to_string(),
                });
            }
            if self.match_ident_str("exception") {
                self.advance();
                exception_block = Some(self.parse_pl_exception_block()?);
            } else if self.peek_keyword() == Some(Keyword::END_P) {
                self.advance();
                break;
            } else {
                let stmt = self.parse_pl_statement()?;
                body.push(stmt);
            }
        }

        let end_label = self.try_parse_pl_label();

        Ok(PlBlock { label, declarations, body, exception_block, end_label })
    }

    fn try_parse_pl_label(&mut self) -> Option<String> {
        if matches!(self.peek(), Token::OpShiftL) {
            self.advance();
            let label = self.parse_identifier().ok()?;
            if matches!(self.peek(), Token::OpShiftR) {
                self.advance();
                return Some(label);
            }
        }
        None
    }

    fn parse_pl_declarations(&mut self) -> Result<Vec<PlDeclaration>, ParserError> {
        let mut decls = Vec::new();
        loop {
            match self.peek_keyword() {
                Some(Keyword::BEGIN_P) | Some(Keyword::END_P) => break,
                _ => {
                    if matches!(self.peek(), Token::Eof) {
                        break;
                    }
                    let decl = self.parse_pl_declaration()?;
                    decls.push(decl);
                }
            }
        }
        Ok(decls)
    }

    fn parse_pl_declaration(&mut self) -> Result<PlDeclaration, ParserError> {
        let name = self.parse_identifier()?;

        if name.eq_ignore_ascii_case("pragma") {
            return Ok(self.parse_pragma_declaration());
        }

        if self.match_ident_str("record") && !self.is_next_token_type_name() {
            self.advance();
            self.try_consume_semicolon();
            return Ok(PlDeclaration::Record(PlRecordDecl { name }));
        }

        // PostgreSQL style: name CURSOR FOR/IS ...
        if self.match_ident_str("cursor") {
            self.advance();
            return self.parse_pl_cursor_decl(name);
        }

        // Oracle style: CURSOR name IS ... (first ident was "CURSOR", read actual name)
        if name.eq_ignore_ascii_case("cursor") {
            let real_name = self.parse_identifier()?;
            return self.parse_pl_cursor_decl(real_name);
        }

        // TYPE name IS RECORD/TABLE/VARRAY/... (first ident was "TYPE", read actual name)
        if name.eq_ignore_ascii_case("type") {
            let real_name = self.parse_identifier()?;
            return self.parse_pl_type_decl_body(real_name);
        }

        self.parse_pl_var_decl(name)
    }

    fn parse_pl_var_decl(&mut self, name: String) -> Result<PlDeclaration, ParserError> {
        let mut constant = false;
        if self.match_ident_str("constant") {
            self.advance();
            constant = true;
        }

        let data_type = self.parse_pl_data_type()?;

        let mut not_null = false;
        if self.peek_keyword() == Some(Keyword::NOT) {
            self.advance();
            self.expect_keyword(Keyword::NULL_P)?;
            not_null = true;
        }

        let default = if self.match_token(&Token::ColonEquals) {
            self.advance();
            Some(self.parse_expr()?)
        } else if self.match_ident_str("default") {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let collate = if self.match_ident_str("collate") {
            self.advance();
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.try_consume_semicolon();

        Ok(PlDeclaration::Variable(PlVarDecl { name, data_type, default, constant, not_null, collate }))
    }

    fn is_next_token_type_name(&self) -> bool {
        matches!(
            self.peek_keyword(),
            Some(Keyword::INT_P)
                | Some(Keyword::INTEGER)
                | Some(Keyword::TEXT_P)
                | Some(Keyword::VARCHAR)
                | Some(Keyword::BOOLEAN_P)
                | Some(Keyword::NUMERIC)
                | Some(Keyword::FLOAT_P)
                | Some(Keyword::DOUBLE_P)
                | Some(Keyword::DATE_P)
                | Some(Keyword::TIMESTAMP)
                | Some(Keyword::TIME)
                | Some(Keyword::CHAR_P)
                | Some(Keyword::BIGINT)
                | Some(Keyword::SMALLINT)
                | Some(Keyword::REAL)
        )
    }

    fn parse_pl_data_type(&mut self) -> Result<PlDataType, ParserError> {
        let mut name = self.parse_identifier()?;

        while self.match_token(&Token::Dot) {
            self.advance();
            name.push('.');
            name.push_str(&self.parse_identifier()?);
        }

        if matches!(self.peek(), Token::Percent) {
            self.advance();
            if self.match_ident_str("rowtype") {
                self.advance();
                return Ok(PlDataType::PercentRowType(name));
            }
            if self.match_ident_str("type") {
                self.advance();
            }
            if let Some(dot_pos) = name.rfind('.') {
                let table = name[..dot_pos].to_string();
                let column = name[dot_pos + 1..].to_string();
                return Ok(PlDataType::PercentType { table, column });
            }
            return Ok(PlDataType::PercentType { table: name, column: String::new() });
        }

        let mut type_name = name;

        // Handle compound type names: "character varying", "double precision"
        let lower = type_name.to_lowercase();
        if lower == "character" && self.match_keyword(Keyword::VARYING) {
            type_name.push(' ');
            type_name.push_str(self.peek_keyword().expect("just matched keyword — peek is non-None").as_str());
            self.advance();
        } else if lower == "double" && self.match_keyword(Keyword::PRECISION) {
            type_name.push(' ');
            type_name.push_str(self.peek_keyword().expect("just matched keyword — peek is non-None").as_str());
            self.advance();
        }

        if self.match_token(&Token::LParen) {
            let mut depth = 1i32;
            type_name.push('(');
            self.advance();
            while depth > 0 && !matches!(self.peek(), Token::Eof) {
                match self.peek() {
                    Token::LParen => {
                        depth += 1;
                        type_name.push('(');
                        self.advance();
                    }
                    Token::RParen => {
                        depth -= 1;
                        if depth >= 0 {
                            type_name.push(')');
                            self.advance();
                        }
                    }
                    Token::Comma => {
                        type_name.push(',');
                        self.advance();
                    }
                    Token::Integer(n) => {
                        type_name.push_str(&n.to_string());
                        self.advance();
                    }
                    Token::Ident(s) => {
                        type_name.push_str(s);
                        self.advance();
                    }
                    Token::Keyword(kw) => {
                        if !type_name.ends_with('(') && !type_name.ends_with(',') {
                            type_name.push(' ');
                        }
                        type_name.push_str(kw.as_str());
                        self.advance();
                    }
                    _ => {
                        let tok_str = self.token_to_string();
                        if !type_name.ends_with('(') && !type_name.ends_with(',') {
                            type_name.push(' ');
                        }
                        type_name.push_str(&tok_str);
                        self.advance();
                    }
                }
            }
        }

        if self.match_token(&Token::LBracket) {
            self.advance();
            type_name.push('[');
            while !self.match_token(&Token::RBracket) && !matches!(self.peek(), Token::Eof) {
                let tok_str = self.token_to_string();
                type_name.push_str(&tok_str);
                self.advance();
            }
            if self.match_token(&Token::RBracket) {
                self.advance();
                type_name.push(']');
            }
        }

        match type_name.to_uppercase().as_str() {
            "RECORD" => Ok(PlDataType::Record),
            "CURSOR" => Ok(PlDataType::Cursor),
            "REFCURSOR" | "SYS_REFCURSOR" => Ok(PlDataType::RefCursor),
            _ => Ok(PlDataType::TypeName(type_name)),
        }
    }

    pub(crate) fn parse_pl_cursor_decl(&mut self, name: String) -> Result<PlDeclaration, ParserError> {
        let mut arguments = Vec::new();
        if self.match_token(&Token::LParen) {
            self.advance();
            if !self.match_token(&Token::RParen) {
                loop {
                    let arg_name = self.parse_identifier()?;
                    let arg_mode = if self.match_ident_str("in") {
                        self.advance();
                        if self.match_ident_str("out") {
                            self.advance();
                            PlArgMode::InOut
                        } else {
                            PlArgMode::In
                        }
                    } else if self.match_ident_str("out") {
                        self.advance();
                        PlArgMode::Out
                    } else {
                        PlArgMode::In
                    };
                    let arg_type = self.parse_pl_data_type()?;
                    arguments.push(PlCursorArg { name: arg_name, data_type: arg_type, mode: arg_mode });
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
        }

        let return_type = if self.match_token(&Token::LParen) {
            let rt = Some(self.parse_pl_data_type()?);
            self.expect_token(&Token::RParen)?;
            rt
        } else {
            None
        };

        if self.match_ident_str("scroll") {
            self.advance();
        } else if self.match_ident_str("no") {
            self.advance();
            self.try_consume_ident_str("scroll");
        }

        // Accept both FOR (PostgreSQL) and IS (Oracle-compatible)
        if !self.match_keyword(Keyword::FOR) && !self.match_keyword(Keyword::IS) {
            return Err(ParserError::UnexpectedToken {
                expected: "FOR or IS".to_string(),
                got: format!("{:?}", self.peek()),
                location: self.current_location(),
            });
        }
        self.advance();

        let (query, parsed_query) = {
            let save_pos = self.pos;
            if let Some(stmt) = self.try_parse_dml_statement() {
                if matches!(self.peek(), Token::Semicolon) {
                    let raw = self.tokens_to_raw_string(save_pos, self.pos);
                    (raw, Some(stmt))
                } else {
                    self.pos = save_pos;
                    (self.skip_to_semicolon_or_keyword(), None)
                }
            } else {
                (self.skip_to_semicolon_or_keyword(), None)
            }
        };
        self.try_consume_semicolon();

        Ok(PlDeclaration::Cursor(PlCursorDecl { name, arguments, return_type, query, parsed_query, scrollable: false }))
    }

    fn parse_pl_type_decl(&mut self, name: String) -> Result<PlDeclaration, ParserError> {
        self.expect_keyword(Keyword::TYPE_P)?;
        self.parse_pl_type_decl_body(name)
    }

    pub(crate) fn parse_pl_type_decl_body(&mut self, name: String) -> Result<PlDeclaration, ParserError> {
        if self.match_ident_str("is") || self.match_ident_str("as") {
            self.advance();
        }

        if self.match_ident_str("record") {
            self.advance();
            self.expect_token(&Token::LParen)?;

            let mut fields = Vec::new();
            if !self.match_token(&Token::RParen) {
                loop {
                    let field_name = self.parse_identifier()?;
                    let field_type = self.parse_pl_data_type()?;
                    fields.push(PlTypeField { name: field_name, data_type: field_type });
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }

            self.expect_token(&Token::RParen)?;
            self.try_consume_semicolon();

            Ok(PlDeclaration::Type(PlTypeDecl::Record { name, fields }))
        } else if self.match_keyword(Keyword::TABLE) {
            self.advance();
            self.expect_keyword(Keyword::OF)?;
            let elem_type = self.parse_pl_data_type()?;
            let mut index_by = None;
            if self.match_keyword(Keyword::INDEX) {
                self.advance();
                self.expect_keyword(Keyword::BY)?;
                index_by = Some(self.parse_pl_data_type()?);
            }
            self.try_consume_semicolon();
            Ok(PlDeclaration::Type(PlTypeDecl::TableOf { name, elem_type, index_by }))
        } else if self.match_ident_str("varray") {
            self.advance();
            self.expect_token(&Token::LParen)?;
            let size = self.parse_expr()?;
            self.expect_token(&Token::RParen)?;
            self.expect_keyword(Keyword::OF)?;
            let elem_type = self.parse_pl_data_type()?;
            self.try_consume_semicolon();
            Ok(PlDeclaration::Type(PlTypeDecl::VarrayOf { name, size: Box::new(size), elem_type }))
        } else if self.match_ident_str("ref") {
            self.advance();
            if self.match_ident_str("cursor") || self.match_keyword(Keyword::CURSOR) {
                self.advance();
            } else {
                return Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "CURSOR after REF".to_string(),
                    got: format!("{:?}", self.peek()),
                });
            }
            self.try_consume_semicolon();
            Ok(PlDeclaration::Type(PlTypeDecl::RefCursor { name }))
        } else {
            Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "RECORD, TABLE, or VARRAY after IS/AS".to_string(),
                got: format!("{:?}", self.peek()),
            })
        }
    }

    pub(crate) fn skip_to_semicolon_or_keyword(&mut self) -> String {
        let mut collected = String::new();
        let mut depth = 0i32;
        let mut case_depth = 0i32;

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
                    if depth > 0 {
                        depth -= 1;
                    }
                    if !collected.is_empty() {
                        collected.push(' ');
                    }
                    collected.push_str(&self.token_to_string());
                    self.advance();
                }
                _ => {
                    if depth == 0 && self.match_ident_str("case") {
                        case_depth += 1;
                    }
                    // Inside a SQL CASE expression, WHEN/THEN/ELSE/END belong to CASE, not PL.
                    if depth == 0 && case_depth > 0 && is_sql_case_terminator(self) {
                        if self.match_ident_str("end") {
                            case_depth -= 1;
                        }
                        if !collected.is_empty() {
                            collected.push(' ');
                        }
                        collected.push_str(&self.token_to_string());
                        self.advance();
                        continue;
                    }
                    if depth == 0 && case_depth == 0 && is_pl_terminator(self) {
                        break;
                    }
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

    fn collect_to_semicolon(&mut self) -> String {
        let mut collected = String::new();
        let mut depth = 0i32;
        loop {
            match self.peek() {
                Token::Eof => break,
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
                    if depth > 0 {
                        depth -= 1;
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

    fn parse_pl_statements_until(&mut self, terminators: &[&str]) -> Result<Vec<PlStatement>, ParserError> {
        let mut stmts = Vec::new();
        loop {
            let is_terminator = terminators.iter().any(|t| self.match_ident_str(t));
            if is_terminator {
                break;
            }
            if self.peek_keyword() == Some(Keyword::END_P) {
                break;
            }
            if matches!(self.peek(), Token::Eof) {
                break;
            }
            let stmt = match self.parse_pl_statement() {
                Ok(s) => s,
                Err(e) => {
                    self.add_error(e);
                    self.skip_to_semicolon_or_keyword();
                    continue;
                }
            };
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    fn parse_pl_statement(&mut self) -> Result<PlStatement, ParserError> {
        let before_pos = self.pos;
        let error_count_before = self.errors.len();
        let label = self.try_parse_pl_label();

        let stmt = if self.match_ident_str("if") {
            self.parse_pl_if()
        } else if self.match_ident_str("case") {
            self.parse_pl_case()
        } else if self.match_ident_str("loop") {
            self.parse_pl_loop()
        } else if self.match_ident_str("while") {
            self.parse_pl_while()
        } else if self.match_ident_str("for") {
            self.parse_pl_for()
        } else if self.match_ident_str("foreach") {
            self.parse_pl_foreach()
        } else if self.match_ident_str("exit") {
            self.parse_pl_exit()
        } else if self.match_ident_str("continue") {
            self.parse_pl_continue()
        } else if self.match_ident_str("return") {
            self.parse_pl_return()
        } else if self.match_ident_str("raise") {
            self.parse_pl_raise()
        } else if self.match_ident_str("execute") {
            self.parse_pl_execute()
        } else if self.match_ident_str("perform") {
            self.parse_pl_perform()
        } else if self.match_ident_str("call") {
            self.parse_pl_call()
        } else if self.match_ident_str("open") {
            self.parse_pl_open()
        } else if self.match_ident_str("fetch") {
            self.parse_pl_fetch()
        } else if self.match_ident_str("close") {
            self.parse_pl_close()
        } else if self.match_ident_str("move") {
            self.parse_pl_move()
        } else if self.match_ident_str("get") {
            self.parse_pl_get_diagnostics()
        } else if self.match_ident_str("commit") {
            self.advance();
            self.try_consume_ident_str("work");
            let and_chain = if self.match_ident_str("and") {
                self.advance();
                self.try_consume_ident_str("chain");
                true
            } else {
                false
            };
            self.try_consume_semicolon();
            Ok(PlStatement::Commit { and_chain })
        } else if self.match_ident_str("rollback") {
            self.parse_pl_rollback()
        } else if self.match_ident_str("savepoint") {
            self.advance();
            let name = self.parse_identifier()?;
            self.try_consume_semicolon();
            Ok(PlStatement::Savepoint { name })
        } else if self.match_ident_str("release") {
            self.advance();
            self.try_consume_ident_str("savepoint");
            let name = self.parse_identifier()?;
            self.try_consume_semicolon();
            Ok(PlStatement::ReleaseSavepoint { name })
        } else if self.match_ident_str("null") {
            self.advance();
            self.try_consume_semicolon();
            Ok(PlStatement::Null)
        } else if self.match_ident_str("goto") {
            self.parse_pl_goto()
        } else if self.match_ident_str("forall") {
            self.parse_pl_forall()
        } else if self.match_ident_str("pipe") {
            self.parse_pl_pipe_row()
        } else if self.match_ident_str("begin") {
            let start = self.current_location();
            self.advance();
            let mut body = Vec::new();
            let mut exception_block = None;
            loop {
                if matches!(self.peek(), Token::Eof) {
                    return Err(ParserError::UnexpectedToken {
                        location: self.current_location(),
                        expected: "END".to_string(),
                        got: "EOF".to_string(),
                    });
                }
                if self.match_ident_str("exception") {
                    self.advance();
                    exception_block = Some(self.parse_pl_exception_block()?);
                } else if self.peek_keyword() == Some(Keyword::END_P) {
                    self.advance();
                    break;
                } else {
                    let stmt = match self.parse_pl_statement() {
                        Ok(s) => s,
                        Err(e) => {
                            self.add_error(e);
                            self.skip_to_semicolon_or_keyword();
                            continue;
                        }
                    };
                    body.push(stmt);
                }
            }
            let end_label = self.try_parse_pl_label();
            self.try_consume_semicolon();
            Ok(PlStatement::Block(Spanned::new(
                PlBlock { label: None, declarations: Vec::new(), body, exception_block, end_label },
                Some(SourceSpan { start, end: self.prev_location() }),
            )))
        } else if self.match_ident_str("declare") {
            let start = self.current_location();
            self.advance();
            let block = self.parse_pl_block_with_declare(label.clone())?;
            let result =
                Ok(PlStatement::Block(Spanned::new(block, Some(SourceSpan { start, end: self.prev_location() }))));
            self.try_consume_semicolon();
            result
        } else if self.match_ident_str("set") {
            let is_set_transaction = self.tokens.get(self.pos + 1).is_some_and(|t| match &t.token {
                Token::Ident(s) => s.eq_ignore_ascii_case("transaction"),
                Token::Keyword(kw) => kw.as_str().eq_ignore_ascii_case("transaction"),
                _ => false,
            });
            if is_set_transaction {
                self.advance();
                self.advance();
                self.parse_pl_set_transaction()
            } else {
                let start = self.current_location();
                self.advance();
                let set_stmt = self.parse_set()?;
                self.try_consume_semicolon();
                Ok(PlStatement::VariableSet(Spanned::new(
                    set_stmt,
                    Some(SourceSpan { start, end: self.prev_location() }),
                )))
            }
        } else if self.match_ident_str("reset") {
            let next_is_lparen = self.tokens.get(self.pos + 1).is_some_and(|t| matches!(t.token, Token::LParen));
            if next_is_lparen {
                let start = self.current_location();
                self.advance();
                if let Some(stmt) = self.try_parse_pl_procedure_call_from_name("reset".to_string()) {
                    self.try_consume_semicolon();
                    Ok(stmt)
                } else {
                    let reset_stmt = self.parse_reset()?;
                    self.try_consume_semicolon();
                    Ok(PlStatement::VariableReset(Spanned::new(
                        reset_stmt,
                        Some(SourceSpan { start, end: self.prev_location() }),
                    )))
                }
            } else {
                let start = self.current_location();
                self.advance();
                let reset_stmt = self.parse_reset()?;
                self.try_consume_semicolon();
                Ok(PlStatement::VariableReset(Spanned::new(
                    reset_stmt,
                    Some(SourceSpan { start, end: self.prev_location() }),
                )))
            }
        } else if let Some(stmt) = self.try_parse_dml_as_pl_statement() {
            Ok(stmt)
        } else if let Some(stmt) = self.try_parse_inline_ddl_as_pl_statement() {
            Ok(stmt)
        } else {
            let is_dml_start = matches!(
                self.peek(),
                Token::Keyword(Keyword::SELECT)
                    | Token::Keyword(Keyword::WITH)
                    | Token::Keyword(Keyword::INSERT)
                    | Token::Keyword(Keyword::UPDATE)
                    | Token::Keyword(Keyword::DELETE_P)
                    | Token::Keyword(Keyword::MERGE)
                    | Token::Hint(_)
            );
            if is_dml_start {
                let had_real_error = self.errors.len() > error_count_before;
                let err_loc = self.current_location();
                let sql = self.collect_to_semicolon();
                self.try_consume_semicolon();
                if !had_real_error {
                    self.add_error(ParserError::UnexpectedToken {
                        location: err_loc,
                        expected: "valid DML statement".to_string(),
                        got: "unparseable DML".to_string(),
                    });
                }
                if sql.is_empty() {
                    Ok(PlStatement::Null)
                } else {
                    Ok(PlStatement::Sql(sql))
                }
            } else {
                self.parse_pl_sql_or_assignment()
            }
        }?;

        if self.pos == before_pos {
            self.advance();
        }

        if label.is_some() {
            Ok(attach_label(stmt, label))
        } else {
            Ok(stmt)
        }
    }

    fn parse_pl_block_with_declare(&mut self, label: Option<String>) -> Result<PlBlock, ParserError> {
        let declarations = self.parse_pl_declarations()?;
        self.expect_keyword(Keyword::BEGIN_P)?;

        let mut body = Vec::new();
        let mut exception_block = None;

        loop {
            if self.match_ident_str("exception") {
                self.advance();
                exception_block = Some(self.parse_pl_exception_block()?);
            } else if self.peek_keyword() == Some(Keyword::END_P) {
                if self.lookahead_is_compound_end() {
                    self.advance();
                    break;
                }
                self.advance();
                break;
            } else {
                let stmt = match self.parse_pl_statement() {
                    Ok(s) => s,
                    Err(e) => {
                        self.add_error(e);
                        self.skip_to_semicolon_or_keyword();
                        continue;
                    }
                };
                body.push(stmt);
            }
        }

        let end_label = self.try_parse_pl_label();
        Ok(PlBlock { label, declarations, body, exception_block, end_label })
    }

    fn try_parse_dml_as_pl_statement(&mut self) -> Option<PlStatement> {
        let is_dml_or_hint = matches!(
            self.peek(),
            Token::Keyword(
                Keyword::SELECT
                    | Keyword::WITH
                    | Keyword::INSERT
                    | Keyword::UPDATE
                    | Keyword::DELETE_P
                    | Keyword::MERGE
            ) | Token::Hint(_)
        );

        if !is_dml_or_hint {
            return None;
        }

        let hints = self.consume_hints();

        let is_dml = matches!(
            self.peek(),
            Token::Keyword(
                Keyword::SELECT
                    | Keyword::WITH
                    | Keyword::INSERT
                    | Keyword::UPDATE
                    | Keyword::DELETE_P
                    | Keyword::MERGE
            )
        );

        if !is_dml {
            return None;
        }

        let save_pos = self.pos;
        let start_pos = self.pos;
        let saved_error_count = self.errors.len();
        let mut first_err: Option<ParserError> = None;
        let result = match self.peek() {
            Token::Keyword(Keyword::SELECT) => {
                self.pl_into_mode = true;
                let result = match self.parse_select_statement() {
                    Ok(mut stmt) => {
                        let mut merged = hints;
                        merged.append(&mut stmt.hints);
                        stmt.hints = merged;
                        Some(crate::ast::Statement::Select(crate::ast::Spanned::new(stmt, None)))
                    }
                    Err(e) => {
                        first_err = Some(e);
                        None
                    }
                };
                self.pl_into_mode = false;
                result
            }
            Token::Keyword(Keyword::WITH) => {
                if self.is_with_dml_at(self.pos) {
                    let with = match self.parse_with_clause() {
                        Ok(Some(w)) => w,
                        _ => {
                            self.pos = save_pos;
                            return None;
                        }
                    };
                    self.pl_into_mode = true;
                    let result = match self.peek_keyword() {
                        Some(Keyword::INSERT) => {
                            self.advance();
                            match self.parse_insert() {
                                Ok(mut stmt) => {
                                    stmt.with = Some(with);
                                    let mut merged = hints;
                                    merged.append(&mut stmt.hints);
                                    stmt.hints = merged;
                                    Some(crate::ast::Statement::Insert(crate::ast::Spanned::new(stmt, None)))
                                }
                                Err(e) => {
                                    first_err = Some(e);
                                    None
                                }
                            }
                        }
                        Some(Keyword::UPDATE) => {
                            self.advance();
                            match self.parse_update() {
                                Ok(mut stmt) => {
                                    stmt.with = Some(with);
                                    let mut merged = hints;
                                    merged.append(&mut stmt.hints);
                                    stmt.hints = merged;
                                    Some(crate::ast::Statement::Update(crate::ast::Spanned::new(stmt, None)))
                                }
                                Err(e) => {
                                    first_err = Some(e);
                                    None
                                }
                            }
                        }
                        Some(Keyword::DELETE_P) => {
                            self.advance();
                            match self.parse_delete() {
                                Ok(mut stmt) => {
                                    stmt.with = Some(with);
                                    let mut merged = hints;
                                    merged.append(&mut stmt.hints);
                                    stmt.hints = merged;
                                    Some(crate::ast::Statement::Delete(crate::ast::Spanned::new(stmt, None)))
                                }
                                Err(e) => {
                                    first_err = Some(e);
                                    None
                                }
                            }
                        }
                        _ => None,
                    };
                    self.pl_into_mode = false;
                    result
                } else {
                    self.pl_into_mode = true;
                    let result = match self.parse_select_statement() {
                        Ok(mut stmt) => {
                            let mut merged = hints;
                            merged.append(&mut stmt.hints);
                            stmt.hints = merged;
                            Some(crate::ast::Statement::Select(crate::ast::Spanned::new(stmt, None)))
                        }
                        Err(e) => {
                            first_err = Some(e);
                            None
                        }
                    };
                    self.pl_into_mode = false;
                    result
                }
            }
            Token::Keyword(Keyword::INSERT) => {
                self.pl_into_mode = true;
                self.advance();
                let result = match self.parse_insert() {
                    Ok(mut stmt) => {
                        let mut merged = hints;
                        merged.append(&mut stmt.hints);
                        stmt.hints = merged;
                        Some(crate::ast::Statement::Insert(crate::ast::Spanned::new(stmt, None)))
                    }
                    Err(e) => {
                        first_err = Some(e);
                        None
                    }
                };
                self.pl_into_mode = false;
                result
            }
            Token::Keyword(Keyword::UPDATE) => {
                self.pl_into_mode = true;
                self.advance();
                let result = match self.parse_update() {
                    Ok(mut stmt) => {
                        let mut merged = hints;
                        merged.append(&mut stmt.hints);
                        stmt.hints = merged;
                        Some(crate::ast::Statement::Update(crate::ast::Spanned::new(stmt, None)))
                    }
                    Err(e) => {
                        first_err = Some(e);
                        None
                    }
                };
                self.pl_into_mode = false;
                result
            }
            Token::Keyword(Keyword::DELETE_P) => {
                self.pl_into_mode = true;
                self.advance();
                let result = match self.parse_delete() {
                    Ok(mut stmt) => {
                        let mut merged = hints;
                        merged.append(&mut stmt.hints);
                        stmt.hints = merged;
                        Some(crate::ast::Statement::Delete(crate::ast::Spanned::new(stmt, None)))
                    }
                    Err(e) => {
                        first_err = Some(e);
                        None
                    }
                };
                self.pl_into_mode = false;
                result
            }
            Token::Keyword(Keyword::MERGE) => {
                self.advance();
                match self.parse_merge() {
                    Ok(mut stmt) => {
                        let mut merged = hints;
                        merged.append(&mut stmt.hints);
                        stmt.hints = merged;
                        Some(crate::ast::Statement::Merge(crate::ast::Spanned::new(stmt, None)))
                    }
                    Err(e) => {
                        first_err = Some(e);
                        None
                    }
                }
            }
            _ => None,
        };

        match result {
            Some(stmt) => {
                let before_dml = self.errors.split_off(saved_error_count);
                let kept: Vec<_> = before_dml
                    .into_iter()
                    .filter(|e| !matches!(e, ParserError::ReservedKeywordAsIdentifier { .. }))
                    .collect();
                self.errors.extend(kept);
                let dml_end_pos = self.pos;
                let had_semicolon = self.match_token(&Token::Semicolon);
                if had_semicolon {
                    self.advance();
                }
                if !had_semicolon && !self.is_pl_boundary() {
                    let loc = self.current_location();
                    let got = format!("{:?}", self.peek());
                    self.add_error(ParserError::UnexpectedToken {
                        location: loc,
                        expected: "end of DML statement".to_string(),
                        got,
                    });
                    let _ = self.skip_to_semicolon_or_keyword();
                }
                let sql_text = self.tokens_to_raw_string(start_pos, dml_end_pos);
                Some(PlStatement::SqlStatement {
                    span: Some(SourceSpan {
                        start: self.tokens.get(start_pos).map(|t| t.location).unwrap_or_default(),
                        end: self.prev_location(),
                    }),
                    sql_text,
                    statement: Box::new(stmt),
                })
            }
            None => {
                self.pos = save_pos;
                self.errors.truncate(saved_error_count);
                if let Some(e) = first_err {
                    self.errors.push(e);
                }
                None
            }
        }
    }

    /// Issue #320: parse inline DDL inside PL bodies into structured AST.
    ///
    /// Gated on DDL-start keywords that the top-level `parse_statement()`
    /// dispatches on. On success wraps the result as
    /// `PlStatement::SqlStatement`; on ANY failure (parse error recorded, or
    /// recovery fallback `Statement::Empty`) restores position and error
    /// state so the caller falls through to `parse_pl_sql_or_assignment`
    /// (raw-string fallback) — preserving pre-#320 behavior exactly.
    ///
    /// Notes:
    /// - Unreserved keywords (ALTER/COMMENT/DROP/LOCK_P/TRUNCATE/VACUUM) are
    ///   legal PL variable names (`lock := 1;`). Those inputs fail DDL parse
    ///   and backtrack cleanly to the assignment path.
    /// - `parse_statement()` consumes the trailing semicolon itself; the
    ///   captured `sql_text` excludes it to match the DML convention.
    /// - Does NOT set `pl_into_mode` (DDL has no INTO) and does NOT consume
    ///   hints (DDL takes no hints).
    /// - Unlike the DML scaffold, no "missing semicolon" diagnostic is emitted
    ///   for lenient DDL parses: pre-#320 such inputs fell through silently as
    ///   raw SQL, and adding a new error here would change accepted inputs.
    fn try_parse_inline_ddl_as_pl_statement(&mut self) -> Option<PlStatement> {
        let is_ddl_start = matches!(
            self.peek(),
            Token::Keyword(
                Keyword::TRUNCATE
                    | Keyword::CREATE
                    | Keyword::ALTER
                    | Keyword::DROP
                    | Keyword::LOCK_P
                    | Keyword::COMMENT
                    | Keyword::ANALYZE
                    | Keyword::VACUUM
                    | Keyword::CLUSTER
                    | Keyword::REINDEX
            )
        );
        if !is_ddl_start {
            return None;
        }

        let save_pos = self.pos;
        let start_pos = self.pos;
        let saved_error_count = self.errors.len();

        match self.parse_statement() {
            Ok(stmt) if self.errors.len() == saved_error_count && !matches!(stmt, crate::ast::Statement::Empty) => {
                // Some dispatch paths (dispatch_create/alter/drop) leave the
                // trailing semicolon unconsumed; consume it so the caller does
                // not see a spurious empty statement.
                self.try_consume_semicolon();
                // parse_statement may have consumed the trailing semicolon;
                // strip all trailing semicolons from sql_text (DML convention;
                // `;;` leaves two consumed semicolons behind).
                let mut end_pos = self.pos;
                while end_pos > start_pos
                    && matches!(self.tokens.get(end_pos - 1).map(|t| &t.token), Some(Token::Semicolon))
                {
                    end_pos -= 1;
                }
                let sql_text = self.tokens_to_raw_string(start_pos, end_pos);
                Some(PlStatement::SqlStatement {
                    span: Some(SourceSpan {
                        start: self.tokens.get(start_pos).map(|t| t.location).unwrap_or_default(),
                        end: self.prev_location(),
                    }),
                    sql_text,
                    statement: Box::new(stmt),
                })
            }
            _ => {
                self.pos = save_pos;
                self.errors.truncate(saved_error_count);
                None
            }
        }
    }

    /// Try to parse a DML statement at current position without consuming trailing semicolon.
    /// On failure, restores position and returns `None`.
    pub(crate) fn try_parse_dml_statement(&mut self) -> Option<Box<crate::ast::Statement>> {
        let save_pos = self.pos;
        let saved_error_count = self.errors.len();

        let result = match self.peek() {
            Token::Keyword(Keyword::SELECT) => match self.parse_select_statement() {
                Ok(stmt) => Some(crate::ast::Statement::Select(crate::ast::Spanned::new(stmt, None))),
                Err(_) => None,
            },
            Token::Keyword(Keyword::WITH) => {
                if self.is_with_dml_at(self.pos) {
                    // WITH ... INSERT/UPDATE/DELETE pattern
                    let with = match self.parse_with_clause() {
                        Ok(Some(w)) => w,
                        _ => {
                            self.pos = save_pos;
                            return None;
                        }
                    };
                    match self.peek_keyword() {
                        Some(Keyword::INSERT) => {
                            self.advance();
                            match self.parse_insert() {
                                Ok(mut stmt) => {
                                    stmt.with = Some(with);
                                    Some(crate::ast::Statement::Insert(crate::ast::Spanned::new(stmt, None)))
                                }
                                Err(_) => {
                                    self.pos = save_pos;
                                    None
                                }
                            }
                        }
                        Some(Keyword::UPDATE) => {
                            self.advance();
                            match self.parse_update() {
                                Ok(mut stmt) => {
                                    stmt.with = Some(with);
                                    Some(crate::ast::Statement::Update(crate::ast::Spanned::new(stmt, None)))
                                }
                                Err(_) => {
                                    self.pos = save_pos;
                                    None
                                }
                            }
                        }
                        Some(Keyword::DELETE_P) => {
                            self.advance();
                            match self.parse_delete() {
                                Ok(mut stmt) => {
                                    stmt.with = Some(with);
                                    Some(crate::ast::Statement::Delete(crate::ast::Spanned::new(stmt, None)))
                                }
                                Err(_) => {
                                    self.pos = save_pos;
                                    None
                                }
                            }
                        }
                        _ => {
                            self.pos = save_pos;
                            None
                        }
                    }
                } else {
                    // Plain WITH ... SELECT (CTE)
                    match self.parse_select_statement() {
                        Ok(stmt) => Some(crate::ast::Statement::Select(crate::ast::Spanned::new(stmt, None))),
                        Err(_) => None,
                    }
                }
            }
            Token::LParen if self.looks_like_parenthesized_query() => match self.parse_select_statement() {
                Ok(stmt) => Some(crate::ast::Statement::Select(crate::ast::Spanned::new(stmt, None))),
                Err(_) => None,
            },
            Token::Keyword(Keyword::INSERT) => {
                self.advance();
                match self.parse_insert() {
                    Ok(stmt) => Some(crate::ast::Statement::Insert(crate::ast::Spanned::new(stmt, None))),
                    Err(_) => None,
                }
            }
            Token::Keyword(Keyword::UPDATE) => {
                self.advance();
                match self.parse_update() {
                    Ok(stmt) => Some(crate::ast::Statement::Update(crate::ast::Spanned::new(stmt, None))),
                    Err(_) => None,
                }
            }
            Token::Keyword(Keyword::DELETE_P) => {
                self.advance();
                match self.parse_delete() {
                    Ok(stmt) => Some(crate::ast::Statement::Delete(crate::ast::Spanned::new(stmt, None))),
                    Err(_) => None,
                }
            }
            Token::Keyword(Keyword::MERGE) => {
                self.advance();
                match self.parse_merge() {
                    Ok(stmt) => Some(crate::ast::Statement::Merge(crate::ast::Spanned::new(stmt, None))),
                    Err(_) => None,
                }
            }
            _ => None,
        };

        match result {
            Some(stmt) => Some(Box::new(stmt)),
            None => {
                self.pos = save_pos;
                self.errors.truncate(saved_error_count);
                None
            }
        }
    }

    fn try_parse_pl_procedure_call(&mut self) -> Option<PlStatement> {
        let start = self.current_location();
        let save = self.pos;
        let saved_error_count = self.errors.len();

        let name = match self.parse_object_name() {
            Ok(n) => n,
            Err(_) => {
                self.pos = save;
                self.errors.truncate(saved_error_count);
                return None;
            }
        };

        if !self.match_token(&Token::LParen) {
            self.pos = save;
            self.errors.truncate(saved_error_count);
            return None;
        }
        self.advance();

        let mut arguments = Vec::new();
        if !self.match_token(&Token::RParen) {
            loop {
                match self.parse_expr() {
                    Ok(arg) => arguments.push(arg),
                    Err(_) => {
                        self.pos = save;
                        self.errors.truncate(saved_error_count);
                        return None;
                    }
                }
                if self.match_token(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if !self.match_token(&Token::RParen) {
            self.pos = save;
            self.errors.truncate(saved_error_count);
            return None;
        }
        self.advance();

        arguments.retain(|a| !matches!(a, Expr::Default));

        let builtin = crate::parser::function_registry::resolve_builtin_meta(&name);
        Some(PlStatement::ProcedureCall(Spanned::new(
            PlProcedureCall { name, arguments, builtin },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn try_parse_pl_procedure_call_from_name(&mut self, name_str: String) -> Option<PlStatement> {
        let start = self.current_location();
        if !self.match_token(&Token::LParen) {
            return None;
        }
        self.advance();

        let mut arguments = Vec::new();
        if !self.match_token(&Token::RParen) {
            loop {
                match self.parse_expr() {
                    Ok(arg) => arguments.push(arg),
                    Err(_) => return None,
                }
                if self.match_token(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if !self.match_token(&Token::RParen) {
            return None;
        }
        self.advance();

        arguments.retain(|a| !matches!(a, Expr::Default));

        let name: crate::ast::ObjectName = vec![name_str.into()];
        let builtin = crate::parser::function_registry::resolve_builtin_meta(&name);
        Some(PlStatement::ProcedureCall(Spanned::new(
            PlProcedureCall { name, arguments, builtin },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn lookahead_is_transaction(&self) -> bool {
        if self.pos + 1 >= self.tokens.len() {
            return false;
        }
        match &self.tokens[self.pos + 1] {
            crate::token::TokenWithSpan { token: Token::Ident(s), .. } => s.eq_ignore_ascii_case("transaction"),
            crate::token::TokenWithSpan { token: Token::Keyword(kw), .. } => {
                kw.as_str().eq_ignore_ascii_case("transaction")
            }
            _ => false,
        }
    }

    fn parse_pl_set_transaction(&mut self) -> Result<PlStatement, ParserError> {
        use crate::ast::plpgsql::PlIsolationLevel;

        let mut isolation_level = None;
        let mut read_only = None;
        let mut deferrable = None;

        if self.match_ident_str("isolation") {
            self.advance();
            self.expect_ident_str("level")?;
            if self.match_ident_str("read") {
                self.advance();
                if self.match_ident_str("committed") {
                    self.advance();
                    isolation_level = Some(PlIsolationLevel::ReadCommitted);
                } else {
                    self.expect_ident_str("uncommitted")?;
                    isolation_level = Some(PlIsolationLevel::ReadCommitted);
                }
            } else if self.match_ident_str("repeatable") {
                self.advance();
                self.expect_ident_str("read")?;
                isolation_level = Some(PlIsolationLevel::RepeatableRead);
            } else if self.match_ident_str("serializable") {
                self.advance();
                isolation_level = Some(PlIsolationLevel::Serializable);
            }
        }

        if self.match_ident_str("read") {
            self.advance();
            if self.match_ident_str("only") {
                self.advance();
                read_only = Some(true);
            } else {
                self.expect_ident_str("write")?;
                read_only = Some(false);
            }
        }

        if self.match_ident_str("not") {
            self.advance();
            self.expect_ident_str("deferrable")?;
            deferrable = Some(false);
        } else if self.match_ident_str("deferrable") {
            self.advance();
            deferrable = Some(true);
        }

        self.try_consume_semicolon();
        Ok(PlStatement::SetTransaction { isolation_level, read_only, deferrable })
    }

    fn parse_pl_sql_or_assignment(&mut self) -> Result<PlStatement, ParserError> {
        let save = self.pos;
        let can_be_assignment_target = matches!(self.peek(), Token::Ident(_) | Token::QuotedIdent(_))
            || matches!(self.peek(), Token::Keyword(kw) if kw.category() == crate::token::keyword::KeywordCategory::Unreserved);
        if can_be_assignment_target {
            let first = self.parse_identifier().unwrap_or_default();
            let mut name_parts = vec![first];
            while self.match_token(&Token::Dot) {
                self.advance();
                name_parts.push(self.parse_identifier().unwrap_or_default());
            }
            if self.match_token(&Token::ColonEquals) {
                self.advance();
                let expression = self.parse_expr()?;
                self.try_consume_semicolon();
                let target = if name_parts.len() == 1 {
                    let name = &name_parts[0];
                    if !self.scope_stack.is_empty() && self.is_var_declared(&name.to_lowercase()) {
                        Expr::PlVariable(name_parts.into_iter().map(|s| s.into()).collect())
                    } else {
                        Expr::ColumnRef(name_parts.into_iter().map(|s| s.into()).collect())
                    }
                } else {
                    Expr::ColumnRef(name_parts.into_iter().map(|s| s.into()).collect())
                };
                return Ok(PlStatement::Assignment { target, expression });
            }
            self.pos = save;
        }

        if let Some(call) = self.try_parse_pl_procedure_call() {
            self.try_consume_semicolon();
            return Ok(call);
        }

        let sql = self.skip_to_semicolon_or_keyword();
        self.try_consume_semicolon();

        if sql.is_empty() {
            return Ok(PlStatement::Null);
        }

        Ok(PlStatement::Sql(sql))
    }

    fn parse_pl_if(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let condition = self.parse_expr()?;
        self.expect_ident_str("then")?;

        let then_stmts = self.parse_pl_statements_until(&["elsif", "else"])?;

        let mut elsifs = Vec::new();
        while self.match_ident_str("elsif") {
            self.advance();
            let elsif_cond = self.parse_expr()?;
            self.expect_ident_str("then")?;
            let elsif_stmts = self.parse_pl_statements_until(&["elsif", "else"])?;
            elsifs.push(PlElsif { condition: elsif_cond, stmts: elsif_stmts });
        }

        let else_stmts = if self.match_ident_str("else") {
            self.advance();
            self.parse_pl_statements_until(&[])?
        } else {
            Vec::new()
        };

        self.expect_keyword(Keyword::END_P)?;
        self.expect_ident_str("if")?;
        self.try_consume_semicolon();

        Ok(PlStatement::If(Spanned::new(
            PlIfStmt { condition, then_stmts, elsifs, else_stmts },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_case(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();

        let expression = if self.match_ident_str("when") { None } else { Some(self.parse_expr()?) };

        let mut whens = Vec::new();
        while self.match_ident_str("when") {
            self.advance();
            let condition = self.parse_expr()?;
            self.expect_ident_str("then")?;
            let stmts = self.parse_pl_statements_until(&["when", "else"])?;
            whens.push(PlCaseWhen { condition, stmts });
        }

        let else_stmts = if self.match_ident_str("else") {
            self.advance();
            self.parse_pl_statements_until(&[])?
        } else {
            Vec::new()
        };

        self.expect_keyword(Keyword::END_P)?;
        self.expect_ident_str("case")?;
        self.try_consume_semicolon();

        Ok(PlStatement::Case(Spanned::new(
            PlCaseStmt { expression, whens, else_stmts },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_loop(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let body = self.parse_pl_statements_until(&[])?;
        self.expect_keyword(Keyword::END_P)?;
        self.expect_ident_str("loop")?;
        let end_label = self.try_parse_pl_label();
        self.try_consume_semicolon();

        Ok(PlStatement::Loop(Spanned::new(
            PlLoopStmt { label: None, body, end_label },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_while(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let condition = self.parse_expr()?;
        self.expect_ident_str("loop")?;
        let body = self.parse_pl_statements_until(&[])?;
        self.expect_keyword(Keyword::END_P)?;
        self.expect_ident_str("loop")?;
        let end_label = self.try_parse_pl_label();
        self.try_consume_semicolon();

        Ok(PlStatement::While(Spanned::new(
            PlWhileStmt { label: None, condition, body, end_label },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_for(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let variable = self.parse_identifier()?;
        self.expect_ident_str("in")?;

        let kind = self.parse_pl_for_kind()?;

        self.expect_ident_str("loop")?;

        // Push implicit scope for loop variable
        self.push_scope();
        self.declare_var(&variable);
        let body = self.parse_pl_statements_until(&[])?;
        self.pop_scope();

        self.expect_keyword(Keyword::END_P)?;
        self.expect_ident_str("loop")?;
        let end_label = self.try_parse_pl_label();
        self.try_consume_semicolon();

        Ok(PlStatement::For(Spanned::new(
            PlForStmt { label: None, variable, kind, body, end_label },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_for_kind(&mut self) -> Result<PlForKind, ParserError> {
        let mut reverse = false;
        if self.match_ident_str("reverse") {
            self.advance();
            reverse = true;
        }

        if self.match_ident_str("execute") || self.match_ident_str("select") {
            let (query, parsed_query) = {
                let save_pos = self.pos;
                if self.match_ident_str("select") {
                    if let Some(stmt) = self.try_parse_dml_statement() {
                        if self.match_ident_str("loop") {
                            let raw = self.tokens_to_raw_string(save_pos, self.pos);
                            (raw, Some(stmt))
                        } else {
                            self.pos = save_pos;
                            self.advance(); // re-consume "select"
                            (self.collect_until_ident_str("loop")?, None)
                        }
                    } else {
                        self.pos = save_pos;
                        self.advance(); // re-consume "select"
                        (self.collect_until_ident_str("loop")?, None)
                    }
                } else {
                    // "execute" — dynamic SQL, skip structured parse
                    (self.collect_until_ident_str("loop")?, None)
                }
            };
            return Ok(PlForKind::Query { query, parsed_query, using_args: Vec::new() });
        }

        // Handle parenthesized query: FOR var IN (SELECT ... | WITH ...) LOOP
        if matches!(self.peek(), Token::LParen) {
            let save_pos = self.pos;
            self.advance(); // consume (

            let is_query = matches!(self.peek_keyword(), Some(Keyword::SELECT) | Some(Keyword::WITH))
                || self.looks_like_parenthesized_query();

            if is_query {
                if let Some(stmt) = self.try_parse_dml_statement() {
                    if matches!(self.peek(), Token::RParen) {
                        self.advance(); // consume )
                        let raw = self.tokens_to_raw_string(save_pos, self.pos);
                        return Ok(PlForKind::Query { query: raw, parsed_query: Some(stmt), using_args: Vec::new() });
                    }
                }
                let err_loc = self.current_location();
                let mut paren_depth = 1i32;
                let scan_start = save_pos + 1;
                let mut scan_pos = scan_start;
                while scan_pos < self.tokens.len() {
                    match &self.tokens[scan_pos].token {
                        Token::LParen => paren_depth += 1,
                        Token::RParen => {
                            paren_depth -= 1;
                            if paren_depth == 0 {
                                let raw = self.tokens_to_raw_string(save_pos, scan_pos + 1);
                                self.pos = scan_pos + 1;
                                self.add_error(ParserError::UnexpectedToken {
                                    location: err_loc,
                                    expected: "valid query in FOR-IN-SELECT".to_string(),
                                    got: "unparseable query".to_string(),
                                });
                                return Ok(PlForKind::Query { query: raw, parsed_query: None, using_args: Vec::new() });
                            }
                        }
                        _ => {}
                    }
                    scan_pos += 1;
                }
            }

            // Backtrack if it wasn't a parenthesized query
            self.pos = save_pos;
        }

        let saved_pos = self.pos;
        if let Ok(name) = self.parse_identifier() {
            if self.match_ident_str("loop") {
                let cursor_name: Expr = if !self.scope_stack.is_empty() && self.is_var_declared(&name.to_lowercase()) {
                    Expr::PlVariable(vec![name.into()])
                } else {
                    Expr::ColumnRef(vec![name.into()])
                };
                return Ok(PlForKind::Cursor { cursor_name, arguments: Vec::new() });
            }
            if matches!(self.peek(), Token::LParen) {
                let cursor_check = self.pos;
                let mut paren_depth = 0i32;
                let mut found_dotdot = false;
                for j in cursor_check..self.tokens.len() {
                    match &self.tokens[j].token {
                        Token::LParen => paren_depth += 1,
                        Token::RParen => {
                            paren_depth -= 1;
                            if paren_depth == 0 {
                                if let Some(next) = self.tokens.get(j + 1) {
                                    if matches!(next.token, Token::DotDot) {
                                        found_dotdot = true;
                                    }
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if found_dotdot {
                    self.pos = saved_pos;
                } else {
                    let mut arguments = Vec::new();
                    self.advance();
                    if !self.match_token(&Token::RParen) {
                        loop {
                            arguments.push(self.parse_expr()?);
                            if self.match_token(&Token::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect_token(&Token::RParen)?;
                    let cursor_name: Expr =
                        if !self.scope_stack.is_empty() && self.is_var_declared(&name.to_lowercase()) {
                            Expr::PlVariable(vec![name.into()])
                        } else {
                            Expr::ColumnRef(vec![name.into()])
                        };
                    return Ok(PlForKind::Cursor { cursor_name, arguments });
                }
            }
            self.pos = saved_pos;
        }

        let low = self.parse_expr()?;
        if matches!(self.peek(), Token::DotDot) {
            self.advance();
        }
        let high = self.parse_expr()?;

        let step = if self.match_ident_str("by") {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        Ok(PlForKind::Range { low, high, step, reverse })
    }

    fn parse_pl_foreach(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let variable = self.parse_identifier()?;
        self.expect_ident_str("in")?;
        self.expect_ident_str("array")?;
        let expression = self.parse_expr()?;

        let slice = if self.match_ident_str("slice") {
            self.advance();
            if let Token::Integer(n) = self.peek().clone() {
                self.advance();
                Some(n as i32)
            } else {
                None
            }
        } else {
            None
        };

        self.expect_ident_str("loop")?;

        // Push implicit scope for loop variable
        self.push_scope();
        self.declare_var(&variable);
        let body = self.parse_pl_statements_until(&[])?;
        self.pop_scope();

        self.expect_keyword(Keyword::END_P)?;
        self.expect_ident_str("loop")?;
        let end_label = self.try_parse_pl_label();
        self.try_consume_semicolon();

        Ok(PlStatement::ForEach(Spanned::new(
            PlForEachStmt { label: None, variable, expression, slice, body, end_label },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_exit(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();

        let label = if !matches!(self.peek(), Token::Semicolon | Token::Eof) && !self.match_ident_str("when") {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let condition = if self.match_ident_str("when") {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.try_consume_semicolon();
        Ok(PlStatement::Exit { label, condition })
    }

    fn parse_pl_continue(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();

        let label = if !matches!(self.peek(), Token::Semicolon | Token::Eof) && !self.match_ident_str("when") {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let condition = if self.match_ident_str("when") {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.try_consume_semicolon();
        Ok(PlStatement::Continue { label, condition })
    }

    fn parse_pl_return(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();

        if self.match_ident_str("next") {
            self.advance();
            let expression = self.parse_expr()?;
            self.try_consume_semicolon();
            return Ok(PlStatement::ReturnNext { expression });
        }

        if self.match_ident_str("query") {
            self.advance();
            if self.match_ident_str("execute") {
                self.advance();
                let dynamic_expr = self.parse_expr()?;
                let mut using_args = Vec::new();
                if self.match_ident_str("using") {
                    self.advance();
                    loop {
                        let mode = if self.match_ident_str("in") {
                            self.advance();
                            if self.match_ident_str("out") {
                                self.advance();
                                PlUsingMode::InOut
                            } else {
                                PlUsingMode::In
                            }
                        } else if self.match_ident_str("out") {
                            self.advance();
                            PlUsingMode::Out
                        } else {
                            PlUsingMode::In
                        };
                        using_args.push(PlUsingArg { mode, argument: self.parse_expr()? });
                        if self.match_token(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.try_consume_semicolon();
                return Ok(PlStatement::ReturnQuery(Spanned::new(
                    PlReturnQueryStmt {
                        query: String::new(),
                        is_dynamic: true,
                        dynamic_expr: Some(dynamic_expr),
                        using_args,
                        parsed_query: None,
                    },
                    Some(SourceSpan { start, end: self.prev_location() }),
                )));
            } else {
                let save_pos = self.pos;
                if let Some(stmt) = self.try_parse_dml_statement() {
                    let raw = self.tokens_to_raw_string(save_pos, self.pos);
                    self.try_consume_semicolon();
                    return Ok(PlStatement::ReturnQuery(Spanned::new(
                        PlReturnQueryStmt {
                            query: raw,
                            is_dynamic: false,
                            dynamic_expr: None,
                            using_args: Vec::new(),
                            parsed_query: Some(stmt),
                        },
                        Some(SourceSpan { start, end: self.prev_location() }),
                    )));
                }
                let expr = self.parse_expr()?;
                self.try_consume_semicolon();
                return Ok(PlStatement::ReturnQuery(Spanned::new(
                    PlReturnQueryStmt {
                        query: String::new(),
                        is_dynamic: false,
                        dynamic_expr: Some(expr),
                        using_args: Vec::new(),
                        parsed_query: None,
                    },
                    Some(SourceSpan { start, end: self.prev_location() }),
                )));
            }
        }

        let expression =
            if matches!(self.peek(), Token::Semicolon | Token::Eof) { None } else { Some(self.parse_expr()?) };

        self.try_consume_semicolon();
        Ok(PlStatement::Return { expression })
    }

    fn parse_pl_raise(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();

        // Form 1: RAISE; (re-raise in exception handler)
        if matches!(self.peek(), Token::Semicolon) {
            self.advance();
            return Ok(PlStatement::Raise(Spanned::new(
                PlRaiseStmt {
                    level: None,
                    message: None,
                    params: Vec::new(),
                    options: Vec::new(),
                    condname: None,
                    sqlstate: None,
                },
                Some(SourceSpan { start, end: self.prev_location() }),
            )));
        }

        let level = if self.match_ident_str("debug") {
            Some(RaiseLevel::Debug)
        } else if self.match_ident_str("log") {
            Some(RaiseLevel::Log)
        } else if self.match_ident_str("info") {
            Some(RaiseLevel::Info)
        } else if self.match_ident_str("notice") {
            Some(RaiseLevel::Notice)
        } else if self.match_ident_str("warning") {
            Some(RaiseLevel::Warning)
        } else if self.match_ident_str("exception") {
            Some(RaiseLevel::Exception)
        } else {
            None
        };

        if level.is_some() {
            self.advance();
        }

        // Form 2: RAISE level; (level-only raise)
        if matches!(self.peek(), Token::Semicolon) && level.is_some() {
            self.advance();
            return Ok(PlStatement::Raise(Spanned::new(
                PlRaiseStmt {
                    level,
                    message: None,
                    params: Vec::new(),
                    options: Vec::new(),
                    condname: None,
                    sqlstate: None,
                },
                Some(SourceSpan { start, end: self.prev_location() }),
            )));
        }

        // Form 3: RAISE condition_name; (condition name without level)
        if level.is_none() {
            let save_pos = self.pos;
            if let Ok(name) = self.parse_identifier() {
                if matches!(self.peek(), Token::Semicolon) {
                    self.try_consume_semicolon();
                    return Ok(PlStatement::Raise(Spanned::new(
                        PlRaiseStmt {
                            level: None,
                            message: None,
                            params: Vec::new(),
                            options: Vec::new(),
                            condname: Some(name),
                            sqlstate: None,
                        },
                        Some(SourceSpan { start, end: self.prev_location() }),
                    )));
                }
            }
            self.pos = save_pos;
        }

        // Form 4: RAISE [level] USING option = expr, ...
        if self.match_ident_str("using") {
            self.advance();
            let options = self.parse_raise_options()?;
            self.try_consume_semicolon();
            return Ok(PlStatement::Raise(Spanned::new(
                PlRaiseStmt { level, message: None, params: Vec::new(), options, condname: None, sqlstate: None },
                Some(SourceSpan { start, end: self.prev_location() }),
            )));
        }

        // Form 5: RAISE [level] 'format', param1, param2 [USING option = expr, ...]
        let msg_start = self.pos;
        let _msg_expr = self.parse_expr()?;
        let message = self.tokens_to_raw_string(msg_start, self.pos);

        let mut params = Vec::new();
        while self.match_token(&Token::Comma) {
            self.advance();
            if self.match_ident_str("using") {
                self.advance();
                break;
            }
            params.push(self.parse_expr()?);
        }

        let options = if self.match_ident_str("using") {
            self.advance();
            self.parse_raise_options()?
        } else {
            Vec::new()
        };

        self.try_consume_semicolon();

        Ok(PlStatement::Raise(Spanned::new(
            PlRaiseStmt { level, message: Some(message), params, options, condname: None, sqlstate: None },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_raise_options(&mut self) -> Result<Vec<RaiseOption>, ParserError> {
        let mut options = Vec::new();
        loop {
            let opt_name = self.parse_identifier()?;
            self.expect_token(&Token::Eq)?;
            let opt_value = self.parse_expr()?;
            options.push(RaiseOption { name: opt_name, value: opt_value });
            if self.match_token(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(options)
    }

    fn parse_pl_execute(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance(); // consume "execute"

        let immediate = self.try_consume_ident_str("immediate");

        let string_expr = self.parse_expr()?;

        let parsed_query = match &string_expr {
            Expr::Literal(Literal::String(s)) => Self::parse_statement_from_str(s),
            Expr::Literal(Literal::DollarString { body, .. }) => Self::parse_statement_from_str(body),
            _ => None,
        };

        let mut into_targets = Vec::new();
        if self.match_ident_str("into") {
            self.advance();
            loop {
                into_targets.push(self.parse_expr()?);
                if self.match_token(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let mut using_args = Vec::new();
        if self.match_ident_str("using") {
            self.advance();
            loop {
                let mode = if self.match_ident_str("in") {
                    self.advance();
                    if self.match_ident_str("out") {
                        self.advance();
                        PlUsingMode::InOut
                    } else {
                        PlUsingMode::In
                    }
                } else if self.match_ident_str("out") {
                    self.advance();
                    PlUsingMode::Out
                } else {
                    PlUsingMode::In
                };
                using_args.push(PlUsingArg { mode, argument: self.parse_expr()? });
                if self.match_token(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.try_consume_semicolon();

        Ok(PlStatement::Execute(Spanned::new(
            PlExecuteStmt { immediate, string_expr, into_targets, using_args, parsed_query },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_perform(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();

        let (query, parsed_query, parsed_expr) = {
            let save_pos = self.pos;
            if let Some(stmt) = self.try_parse_dml_statement() {
                if matches!(self.peek(), Token::Semicolon) {
                    let raw = self.tokens_to_raw_string(save_pos, self.pos);
                    (raw, Some(stmt), None)
                } else {
                    self.pos = save_pos;
                    match self.parse_expr() {
                        Ok(expr) => {
                            let raw = self.tokens_to_raw_string(save_pos, self.pos);
                            (raw, None, Some(Box::new(expr)))
                        }
                        Err(_) => {
                            self.pos = save_pos;
                            (self.skip_to_semicolon_or_keyword(), None, None)
                        }
                    }
                }
            } else {
                self.pos = save_pos;
                match self.parse_expr() {
                    Ok(expr) => {
                        let raw = self.tokens_to_raw_string(save_pos, self.pos);
                        (raw, None, Some(Box::new(expr)))
                    }
                    Err(_) => {
                        self.pos = save_pos;
                        (self.skip_to_semicolon_or_keyword(), None, None)
                    }
                }
            }
        };
        self.try_consume_semicolon();

        Ok(PlStatement::Perform {
            span: Some(SourceSpan { start, end: self.prev_location() }),
            query,
            parsed_query,
            parsed_expr,
        })
    }

    fn parse_pl_call(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        let save_pos = self.pos;

        self.advance();

        let name = match self.parse_object_name() {
            Ok(n) => n,
            Err(_) => {
                self.pos = save_pos;
                let sql = self.skip_to_semicolon_or_keyword();
                return Ok(PlStatement::Sql(sql));
            }
        };

        if self.expect_token(&Token::LParen).is_err() {
            self.pos = save_pos;
            let sql = self.skip_to_semicolon_or_keyword();
            return Ok(PlStatement::Sql(sql));
        }

        let mut arguments = Vec::new();
        if !self.match_token(&Token::RParen) {
            loop {
                match self.parse_expr() {
                    Ok(arg) => arguments.push(arg),
                    Err(_) => {
                        self.pos = save_pos;
                        let sql = self.skip_to_semicolon_or_keyword();
                        return Ok(PlStatement::Sql(sql));
                    }
                }
                if self.match_token(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if self.expect_token(&Token::RParen).is_err() {
            self.pos = save_pos;
            let sql = self.skip_to_semicolon_or_keyword();
            return Ok(PlStatement::Sql(sql));
        }

        arguments.retain(|a| !matches!(a, Expr::Default));

        self.try_consume_semicolon();

        let builtin = crate::parser::function_registry::resolve_builtin_meta(&name);
        Ok(PlStatement::ProcedureCall(Spanned::new(
            PlProcedureCall { name, arguments, builtin },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_open(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let cursor = self.parse_expr()?;

        let kind = if self.match_token(&Token::LParen) {
            self.advance();
            let mut arguments = Vec::new();
            if !self.match_token(&Token::RParen) {
                loop {
                    arguments.push(self.parse_expr()?);
                    if self.match_token(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(&Token::RParen)?;
            PlOpenKind::Simple { arguments }
        } else {
            let scroll = if self.match_ident_str("scroll") {
                self.advance();
                Some(true)
            } else if self.match_ident_str("no") {
                self.advance();
                if self.match_ident_str("scroll") {
                    self.advance();
                }
                Some(false)
            } else {
                None
            };

            if self.match_ident_str("for") {
                self.advance();
                if self.match_ident_str("execute") {
                    self.advance();
                    let query = self.parse_expr()?;
                    let mut using_args = Vec::new();
                    if self.match_ident_str("using") {
                        self.advance();
                        loop {
                            using_args.push(self.parse_expr()?);
                            if self.match_token(&Token::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    PlOpenKind::ForExecute { query, using_args }
                } else if self.match_ident_str("using") {
                    self.advance();
                    let mut expressions = Vec::new();
                    loop {
                        expressions.push(self.parse_expr()?);
                        if self.match_token(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    PlOpenKind::ForUsing { expressions }
                } else {
                    let (query, parsed_query) = {
                        let save_pos = self.pos;
                        if let Some(stmt) = self.try_parse_dml_statement() {
                            if matches!(self.peek(), Token::Semicolon) {
                                let raw = self.tokens_to_raw_string(save_pos, self.pos);
                                (raw, Some(stmt))
                            } else {
                                self.pos = save_pos;
                                (self.skip_to_semicolon_or_keyword(), None)
                            }
                        } else {
                            (self.skip_to_semicolon_or_keyword(), None)
                        }
                    };
                    PlOpenKind::ForQuery { scroll, query, parsed_query }
                }
            } else {
                PlOpenKind::Simple { arguments: Vec::new() }
            }
        };

        self.try_consume_semicolon();
        Ok(PlStatement::Open(Spanned::new(
            PlOpenStmt { cursor, kind },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_fetch(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();

        let direction = if self.match_ident_str("next")
            || self.match_ident_str("prior")
            || self.match_ident_str("first")
            || self.match_ident_str("last")
            || self.match_ident_str("forward")
            || self.match_ident_str("backward")
            || self.match_ident_str("absolute")
            || self.match_ident_str("relative")
            || self.match_ident_str("all")
        {
            let dir = self.parse_pl_cursor_direction()?;
            if self.match_ident_str("from") || self.match_ident_str("in") {
                self.advance();
            }
            Some(dir)
        } else {
            None
        };

        let cursor = self.parse_expr()?;

        let bulk_collect = if self.match_ident_str("bulk") {
            self.advance();
            self.expect_ident_str("collect")?;
            true
        } else {
            false
        };

        self.expect_ident_str("into")?;
        let mut into_vars = vec![self.parse_expr()?];
        while self.match_token(&Token::Comma) {
            self.advance();
            into_vars.push(self.parse_expr()?);
        }
        self.try_consume_semicolon();

        Ok(PlStatement::Fetch(Spanned::new(
            PlFetchStmt { cursor, direction, bulk_collect, into: into_vars },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_cursor_direction(&mut self) -> Result<FetchDirection, ParserError> {
        let dir_str = self.token_to_string();
        self.advance();
        match dir_str.to_uppercase().as_str() {
            "NEXT" => Ok(FetchDirection::Next),
            "PRIOR" => Ok(FetchDirection::Prior),
            "FIRST" => Ok(FetchDirection::First),
            "LAST" => Ok(FetchDirection::Last),
            "ABSOLUTE" => {
                let n = self.parse_pl_signed_integer();
                Ok(FetchDirection::Absolute(n))
            }
            "RELATIVE" => {
                let n = self.parse_pl_signed_integer();
                Ok(FetchDirection::Relative(n))
            }
            "FORWARD" => {
                if self.match_ident_str("all") {
                    self.advance();
                    Ok(FetchDirection::ForwardAll)
                } else if let Some(n) = self.try_parse_pl_integer() {
                    Ok(FetchDirection::Forward(Some(n)))
                } else {
                    Ok(FetchDirection::Forward(None))
                }
            }
            "BACKWARD" => {
                if self.match_ident_str("all") {
                    self.advance();
                    Ok(FetchDirection::BackwardAll)
                } else if let Some(n) = self.try_parse_pl_integer() {
                    Ok(FetchDirection::Backward(Some(n)))
                } else {
                    Ok(FetchDirection::Backward(None))
                }
            }
            "ALL" => Ok(FetchDirection::All),
            _ => unreachable!("invalid fetch direction: {}", dir_str),
        }
    }

    fn parse_pl_signed_integer(&mut self) -> i64 {
        let neg = if matches!(self.peek(), Token::Minus) {
            self.advance();
            true
        } else {
            false
        };
        let n = if let Token::Integer(i) = self.peek().clone() {
            self.advance();
            i
        } else {
            0
        };
        if neg {
            -n
        } else {
            n
        }
    }

    fn try_parse_pl_integer(&mut self) -> Option<i64> {
        if let Token::Integer(i) = self.peek().clone() {
            self.advance();
            Some(i)
        } else {
            None
        }
    }

    fn parse_pl_close(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();
        let cursor = self.parse_expr()?;
        self.try_consume_semicolon();

        Ok(PlStatement::Close { cursor })
    }

    fn parse_pl_move(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();

        let direction = if self.match_ident_str("next")
            || self.match_ident_str("prior")
            || self.match_ident_str("first")
            || self.match_ident_str("last")
            || self.match_ident_str("forward")
            || self.match_ident_str("backward")
            || self.match_ident_str("absolute")
            || self.match_ident_str("relative")
            || self.match_ident_str("all")
        {
            let dir = self.parse_pl_cursor_direction()?;
            if self.match_ident_str("from") || self.match_ident_str("in") {
                self.advance();
            }
            Some(dir)
        } else {
            None
        };

        let cursor = self.parse_expr()?;
        self.try_consume_semicolon();

        Ok(PlStatement::Move { cursor, direction })
    }

    fn parse_pl_get_diagnostics(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();

        let stacked = if self.match_ident_str("stacked") {
            self.advance();
            true
        } else {
            false
        };

        self.expect_ident_str("diagnostics")?;

        let mut items = Vec::new();
        loop {
            let target_name = self.parse_identifier()?;
            let target: Expr = if !self.scope_stack.is_empty() && self.is_var_declared(&target_name.to_lowercase()) {
                Expr::PlVariable(vec![target_name.into()])
            } else {
                Expr::ColumnRef(vec![target_name.into()])
            };
            self.expect_token(&Token::Eq)?;
            let item_str = self.parse_identifier()?;
            let item = match item_str.to_uppercase().as_str() {
                "ROW_COUNT" => GetDiagItemKind::RowCount,
                "RESULT_STATUS" => GetDiagItemKind::ResultStatus,
                "RETURNED_SQLSTATE" => GetDiagItemKind::ReturnedSqlstate,
                "MESSAGE_TEXT" => GetDiagItemKind::MessageText,
                "DETAIL" => GetDiagItemKind::Detail,
                "HINT" => GetDiagItemKind::Hint,
                "CONTEXT" => GetDiagItemKind::Context,
                "SCHEMA_NAME" => GetDiagItemKind::SchemaName,
                "TABLE_NAME" => GetDiagItemKind::TableName,
                "COLUMN_NAME" => GetDiagItemKind::ColumnName,
                "DATATYPE_NAME" => GetDiagItemKind::DatatypeName,
                "CONSTRAINT_NAME" => GetDiagItemKind::ConstraintName,
                "PG_EXCEPTION_CONTEXT" => GetDiagItemKind::PgExceptionContext,
                _ => {
                    return Err(ParserError::UnexpectedToken {
                        location: self.current_location(),
                        expected: "known GET DIAGNOSTICS item".to_string(),
                        got: item_str,
                    });
                }
            };
            items.push(PlGetDiagItem { target, item });

            if self.match_token(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.try_consume_semicolon();
        Ok(PlStatement::GetDiagnostics(Spanned::new(
            PlGetDiagStmt { stacked, items },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_rollback(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();
        self.try_consume_ident_str("work");

        let to_savepoint = if self.match_ident_str("to") {
            self.advance();
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let and_chain = if self.match_ident_str("and") {
            self.advance();
            self.try_consume_ident_str("chain");
            true
        } else {
            false
        };

        self.try_consume_semicolon();
        Ok(PlStatement::Rollback { to_savepoint, and_chain })
    }

    fn parse_pl_goto(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();
        let label = self.parse_identifier()?;
        self.try_consume_semicolon();
        Ok(PlStatement::Goto { label })
    }

    fn parse_pl_forall(&mut self) -> Result<PlStatement, ParserError> {
        let start = self.current_location();
        self.advance();
        let variable = self.parse_identifier()?;
        self.expect_ident_str("in")?;

        let mut bounds = String::new();
        let mut save_exceptions = false;
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
                    if !bounds.is_empty() {
                        bounds.push(' ');
                    }
                    bounds.push_str(&self.token_to_string());
                    self.advance();
                }
                Token::RParen => {
                    if depth > 0 {
                        depth -= 1;
                    }
                    if !bounds.is_empty() {
                        bounds.push(' ');
                    }
                    bounds.push_str(&self.token_to_string());
                    self.advance();
                }
                _ => {
                    if depth == 0 && self.match_ident_str("save") {
                        let save_pos = self.pos;
                        self.advance();
                        if self.match_ident_str("exceptions") {
                            self.advance();
                            save_exceptions = true;
                            continue;
                        } else {
                            self.pos = save_pos;
                        }
                    }
                    if depth == 0 && is_pl_terminator(self) {
                        break;
                    }
                    if !bounds.is_empty() {
                        bounds.push(' ');
                    }
                    bounds.push_str(&self.token_to_string());
                    self.advance();
                }
            }
        }

        self.try_consume_semicolon();

        Ok(PlStatement::ForAll(Spanned::new(
            PlForAllStmt { variable, bounds: bounds.trim().to_string(), save_exceptions, body: String::new() },
            Some(SourceSpan { start, end: self.prev_location() }),
        )))
    }

    fn parse_pl_pipe_row(&mut self) -> Result<PlStatement, ParserError> {
        self.advance();
        self.expect_ident_str("row")?;
        let expression = self.parse_expr()?;
        self.try_consume_semicolon();

        Ok(PlStatement::PipeRow { expression })
    }

    fn parse_pl_exception_block(&mut self) -> Result<PlExceptionBlock, ParserError> {
        let mut handlers = Vec::new();
        loop {
            if !self.match_ident_str("when") {
                break;
            }
            self.advance();

            let mut conditions = Vec::new();
            loop {
                let cond = self.parse_identifier()?;
                let cond = if cond.eq_ignore_ascii_case("sqlstate") {
                    if let Token::StringLiteral(s) = self.peek().clone() {
                        self.advance();
                        format!("SQLSTATE '{}'", s)
                    } else {
                        cond
                    }
                } else {
                    cond
                };
                conditions.push(cond);
                if !self.match_ident_str("or") {
                    break;
                }
                self.advance();
            }

            self.expect_ident_str("then")?;
            let statements = self.parse_pl_statements_until(&["when"])?;

            handlers.push(PlExceptionHandler { conditions, statements });
        }
        Ok(PlExceptionBlock { handlers })
    }

    fn collect_until_ident_str(&mut self, target: &str) -> Result<String, ParserError> {
        let mut collected = String::new();
        let mut depth = 0i32;

        loop {
            if depth == 0 && self.match_ident_str(target) {
                break;
            }
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
                    if depth > 0 {
                        depth -= 1;
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

        Ok(collected.trim().to_string())
    }

    fn collect_until_token(&mut self, token: &Token) -> String {
        let mut collected = String::new();
        let mut depth = 0i32;

        loop {
            if self.peek() == token && depth == 0 {
                break;
            }
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
                    if depth > 0 {
                        depth -= 1;
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

    fn skip_to_comma_or_rparen(&mut self) -> String {
        let mut collected = String::new();
        let mut depth = 0i32;

        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Comma if depth == 0 => break,
                Token::RParen if depth == 0 => break,
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
}

fn is_pl_terminator(p: &Parser) -> bool {
    let terminators = ["begin", "end", "exception", "when", "then", "else", "elsif", "loop", "declare"];
    terminators.iter().any(|t| p.match_ident_str(t))
}

fn is_sql_case_terminator(p: &Parser) -> bool {
    let case_terms = ["when", "then", "else", "end"];
    case_terms.iter().any(|t| p.match_ident_str(t))
}

fn attach_label(stmt: PlStatement, label: Option<String>) -> PlStatement {
    let label = match label {
        Some(l) => l,
        None => return stmt,
    };
    match stmt {
        PlStatement::Loop(mut s) => {
            s.node.label = Some(label);
            PlStatement::Loop(s)
        }
        PlStatement::While(mut s) => {
            s.node.label = Some(label);
            PlStatement::While(s)
        }
        PlStatement::For(mut s) => {
            s.node.label = Some(label);
            PlStatement::For(s)
        }
        PlStatement::ForEach(mut s) => {
            s.node.label = Some(label);
            PlStatement::ForEach(s)
        }
        PlStatement::Block(mut s) => {
            s.node.label = Some(label);
            PlStatement::Block(s)
        }
        // Standalone label before non-control statement: wrap in a labeled Block
        // so the label is preserved in the AST for GOTO target analysis.
        other => PlStatement::Block(Spanned::new(
            PlBlock {
                label: Some(label),
                declarations: Vec::new(),
                body: vec![other],
                exception_block: None,
                end_label: None,
            },
            None,
        )),
    }
}

impl Parser {
    pub(crate) fn parse_procedure_body(&mut self, param_names: &[String]) -> Result<PlBlock, ParserError> {
        self.push_scope();
        for name in param_names {
            self.declare_var(name);
        }
        // Pair push/pop around the fallible inner parse (same pattern as
        // parse_pl_block_body): `?` early-returns inside must not leak the
        // pushed scope frame, or backtracking callers (e.g. the inline-DDL
        // helper, issue #320) would leave phantom variable declarations.
        let result = self.parse_procedure_body_inner();
        self.pop_scope();
        result
    }

    fn parse_procedure_body_inner(&mut self) -> Result<PlBlock, ParserError> {
        let mut declarations = Vec::new();

        while !self.match_keyword(Keyword::BEGIN_P) && !matches!(self.peek(), Token::Eof) {
            if self.match_ident_str("procedure") {
                self.advance();
                let sp_start = self.pos.saturating_sub(1);
                match self.parse_package_sub_procedure(sp_start) {
                    Ok(proc) => declarations.push(PlDeclaration::NestedProcedure(proc)),
                    Err(_) => self.advance(),
                }
            } else if self.match_ident_str("function") {
                self.advance();
                let sp_start = self.pos.saturating_sub(1);
                match self.parse_package_sub_function(sp_start) {
                    Ok(func) => declarations.push(PlDeclaration::NestedFunction(func)),
                    Err(_) => self.advance(),
                }
            } else if self.match_ident_str("pragma") {
                self.advance();
                let pragma = self.parse_pragma_declaration();
                declarations.push(pragma);
            } else if self.match_ident_str("cursor") {
                self.advance();
                let cursor_name = self.parse_identifier()?;
                let decl = self.parse_pl_cursor_decl(cursor_name)?;
                declarations.push(decl);
            } else if self.peek_keyword() == Some(Keyword::TYPE_P) {
                self.advance();
                let type_name = self.parse_identifier()?;
                match self.parse_pl_type_decl_body(type_name) {
                    Ok(decl) => declarations.push(decl),
                    Err(_) => self.advance(),
                }
            } else if let Some(decl) = self.try_parse_oracle_var_decl() {
                declarations.push(decl);
            } else {
                self.advance();
            }
        }

        self.expect_keyword(Keyword::BEGIN_P)?;

        for decl in &declarations {
            if let Some(name) = self.declaration_name(decl) {
                self.declare_var(&name);
            }
        }

        let mut body = Vec::new();
        let mut exception_block = None;

        loop {
            if self.match_ident_str("exception") {
                self.advance();
                exception_block = Some(self.parse_pl_exception_block()?);
            } else if self.peek_keyword() == Some(Keyword::END_P) {
                if self.lookahead_is_compound_end() {
                    return Err(ParserError::UnexpectedToken {
                        location: self.current_location(),
                        expected: "end of procedure body".to_string(),
                        got: format!("{:?}", self.peek()),
                    });
                }
                self.advance();
                while matches!(self.peek(), Token::Ident(_)) {
                    self.advance();
                }
                self.try_consume_semicolon();
                break;
            } else if matches!(self.peek(), Token::Eof) {
                break;
            } else {
                let stmt = match self.parse_pl_statement() {
                    Ok(s) => s,
                    Err(e) => {
                        self.add_error(e);
                        self.skip_to_semicolon_or_keyword();
                        continue;
                    }
                };
                body.push(stmt);
            }
        }

        Ok(PlBlock { label: None, declarations, body, exception_block, end_label: None })
    }

    pub(crate) fn parse_pl_declarations_until_begin(&mut self) -> Result<Vec<PlDeclaration>, ParserError> {
        let mut declarations = Vec::new();
        while !self.match_keyword(Keyword::BEGIN_P) && !matches!(self.peek(), Token::Eof) {
            if self.match_ident_str("procedure") {
                self.advance();
                let sp_start = self.pos.saturating_sub(1);
                match self.parse_package_sub_procedure(sp_start) {
                    Ok(proc) => declarations.push(PlDeclaration::NestedProcedure(proc)),
                    Err(_) => self.advance(),
                }
            } else if self.match_ident_str("function") {
                self.advance();
                let sp_start = self.pos.saturating_sub(1);
                match self.parse_package_sub_function(sp_start) {
                    Ok(func) => declarations.push(PlDeclaration::NestedFunction(func)),
                    Err(_) => self.advance(),
                }
            } else if self.match_ident_str("pragma") {
                self.advance();
                let pragma = self.parse_pragma_declaration();
                declarations.push(pragma);
            } else if self.match_keyword(Keyword::TYPE_P) {
                self.advance();
                let type_name = self.parse_identifier()?;
                match self.parse_pl_type_decl_body(type_name) {
                    Ok(decl) => declarations.push(decl),
                    Err(_) => self.advance(),
                }
            } else if self.match_ident_str("cursor") {
                self.advance();
                let cursor_name = self.parse_identifier()?;
                let decl = self.parse_pl_cursor_decl(cursor_name)?;
                declarations.push(decl);
            } else if let Some(decl) = self.try_parse_oracle_var_decl() {
                declarations.push(decl);
            } else {
                self.advance();
            }
        }
        self.expect_keyword(Keyword::BEGIN_P)?;
        Ok(declarations)
    }

    pub(crate) fn try_parse_oracle_var_decl(&mut self) -> Option<PlDeclaration> {
        let is_unreserved_kw = matches!(self.peek(), Token::Keyword(kw) if kw.category() == crate::token::keyword::KeywordCategory::Unreserved);
        if !matches!(self.peek(), Token::Ident(_)) && !is_unreserved_kw {
            return None;
        }

        let start_pos = self.pos;

        let name = match self.peek() {
            Token::Ident(s) => s.clone(),
            Token::Keyword(kw) => kw.as_str().to_string(),
            _ => return None,
        };

        if name.eq_ignore_ascii_case("begin")
            || name.eq_ignore_ascii_case("end")
            || name.eq_ignore_ascii_case("procedure")
            || name.eq_ignore_ascii_case("function")
            || name.eq_ignore_ascii_case("exception")
            || name.eq_ignore_ascii_case("declare")
            || name.eq_ignore_ascii_case("cursor")
            || name.eq_ignore_ascii_case("type")
            || name.eq_ignore_ascii_case("pragma")
        {
            return None;
        }

        self.advance();

        if self.match_ident_str("cursor") {
            self.advance();
            return match self.parse_pl_cursor_decl(name) {
                Ok(decl) => Some(decl),
                Err(_) => {
                    self.pos = start_pos;
                    None
                }
            };
        }

        if self.match_keyword(Keyword::IS) || self.match_keyword(Keyword::AS) {
            self.pos = start_pos;
            return None;
        }

        if self.match_keyword(Keyword::TYPE_P) {
            self.advance();
            return match self.parse_pl_type_decl_body(name) {
                Ok(decl) => Some(decl),
                Err(_) => {
                    self.pos = start_pos;
                    None
                }
            };
        }

        let mut constant = false;
        if self.match_ident_str("constant") {
            self.advance();
            constant = true;
        }

        let data_type = match self.parse_pl_data_type() {
            Ok(dt) => dt,
            Err(_) => {
                self.pos = start_pos;
                return None;
            }
        };

        let mut not_null = false;
        if self.peek_keyword() == Some(Keyword::NOT) {
            let saved_pos = self.pos;
            self.advance();
            if self.match_keyword(Keyword::NULL_P) {
                self.advance();
                not_null = true;
            } else {
                self.pos = saved_pos;
            }
        }

        let default = if self.match_token(&Token::ColonEquals) {
            self.advance();
            match self.parse_expr() {
                Ok(e) => Some(e),
                Err(_) => {
                    self.pos = start_pos;
                    return None;
                }
            }
        } else if self.match_ident_str("default") {
            self.advance();
            match self.parse_expr() {
                Ok(e) => Some(e),
                Err(_) => {
                    self.pos = start_pos;
                    return None;
                }
            }
        } else {
            None
        };

        self.try_consume_semicolon();

        Some(PlDeclaration::Variable(PlVarDecl { name, data_type, default, constant, not_null, collate: None }))
    }

    fn parse_pragma_declaration(&mut self) -> PlDeclaration {
        let name = match self.peek() {
            Token::Ident(s) => {
                let n = s.clone();
                self.advance();
                n
            }
            _ => return PlDeclaration::Pragma { name: String::new(), arguments: String::new() },
        };

        let arguments = if self.match_token(&Token::LParen) {
            self.advance();
            let mut depth = 0i32;
            let mut args = String::new();
            loop {
                match self.peek() {
                    Token::Eof => break,
                    Token::RParen => {
                        if depth == 0 {
                            self.advance();
                            break;
                        }
                        depth -= 1;
                        args.push(')');
                        self.advance();
                    }
                    Token::LParen => {
                        depth += 1;
                        args.push('(');
                        self.advance();
                    }
                    _ => {
                        if !args.is_empty() {
                            args.push(' ');
                        }
                        args.push_str(&self.token_to_string());
                        self.advance();
                    }
                }
            }
            args
        } else {
            String::new()
        };

        self.try_consume_semicolon();

        PlDeclaration::Pragma { name, arguments }
    }
}
