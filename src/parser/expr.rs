use crate::ast::{
    DataType, Expr, Literal, ObjectName, OrderByItem, SelectStatement, SelectTarget, SequenceFunc, WhenClause,
    WindowFrame, WindowFrameBound, WindowFrameDirection, WindowFrameMode, WindowSpec, XmlAttribute, XmlAttributes,
    XmlContent, XmlOption,
};
use crate::parser::{Parser, ParserError};
use crate::token::keyword::Keyword;
use crate::token::Token;

impl Parser {
    fn validate_func(
        &mut self,
        name: &ObjectName,
        arg_count: usize,
        distinct: bool,
        has_over: bool,
        has_variadic: bool,
    ) -> Option<crate::ast::BuiltinFuncMeta> {
        let builtin = crate::parser::function_registry::resolve_builtin_meta(name);

        let full_name = name.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>().join(".");
        let last_seg = full_name.split('.').next_back().unwrap_or(&full_name).to_string();

        let warnings = crate::parser::function_registry::validate_function_call(
            &last_seg,
            arg_count,
            distinct,
            has_over,
            has_variadic,
            self.current_location(),
        );
        for w in warnings {
            self.add_error(w);
        }
        builtin
    }

    pub(crate) fn parse_expr(&mut self) -> Result<Expr, ParserError> {
        self.enter_scope()?;
        let result = self.parse_expr_with_precedence(0);
        self.leave_scope();
        result
    }

    fn parse_expr_with_precedence(&mut self, min_prec: u8) -> Result<Expr, ParserError> {
        let mut left = self.parse_unary_expr()?;

        loop {
            // Try postfix operators first — they bind tighter than any infix operator.
            // This must be inside the loop so that after consuming e.g. "IS NULL",
            // we continue and can still pick up subsequent infix operators like "OR".
            if self.try_postfix_op(&mut left)? {
                continue;
            }

            let (op_prec, op_str, is_right_assoc) = match self.infix_operator() {
                Some(info) => info,
                None => break,
            };

            if op_prec < min_prec {
                break;
            }

            self.advance();

            if op_str == "::" {
                let type_name = self.parse_data_type()?;
                left = Expr::TypeCast { expr: Box::new(left), type_name, default: None, format: None };
                continue;
            }

            if matches!(op_str.as_str(), "=" | "<" | ">" | "<=" | ">=" | "<>" | "!=")
                && matches!(
                    self.peek(),
                    Token::Keyword(Keyword::ANY) | Token::Keyword(Keyword::SOME) | Token::Keyword(Keyword::ALL)
                )
            {
                let sublink_type = match self.peek() {
                    Token::Keyword(Keyword::ANY) => crate::ast::ScalarSublinkType::Any,
                    Token::Keyword(Keyword::SOME) => crate::ast::ScalarSublinkType::Some,
                    Token::Keyword(Keyword::ALL) => crate::ast::ScalarSublinkType::All,
                    _ => unreachable!(),
                };
                self.advance();
                self.expect_token(&Token::LParen)?;
                self.consume_hints();
                if self.match_keyword(Keyword::VALUES) {
                    self.advance();
                    let values_stmt = self.parse_values_statement()?;
                    self.expect_token(&Token::RParen)?;
                    let targets = values_stmt
                        .rows
                        .into_iter()
                        .filter_map(|mut row| row.pop())
                        .map(|expr| crate::ast::SelectTarget::Expr(expr, None))
                        .collect();
                    left = Expr::ScalarSublink {
                        expr: Box::new(left),
                        op: op_str,
                        sublink_type,
                        subquery: Box::new(crate::ast::SelectStatement {
                            targets,
                            hints: vec![],
                            with: None,
                            distinct: false,
                            distinct_on: vec![],
                            into_targets: None,
                            bulk_collect: false,
                            into_table: None,
                            from: vec![],
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
                        }),
                    };
                    continue;
                }
                let saved_pos = self.pos;
                let saved_err_len = self.errors.len();
                if let Ok(subquery) = self.parse_select_statement() {
                    self.expect_token(&Token::RParen)?;
                    left = Expr::ScalarSublink {
                        expr: Box::new(left),
                        op: op_str,
                        sublink_type,
                        subquery: Box::new(subquery),
                    };
                    continue;
                }
                self.pos = saved_pos;
                self.errors.truncate(saved_err_len);
                let expr_val = self.parse_expr()?;
                self.expect_token(&Token::RParen)?;
                left = Expr::ScalarSublink {
                    expr: Box::new(left),
                    op: op_str,
                    sublink_type,
                    subquery: Box::new(crate::ast::SelectStatement {
                        targets: vec![crate::ast::SelectTarget::Expr(expr_val, None)],
                        hints: vec![],
                        with: None,
                        distinct: false,
                        distinct_on: vec![],
                        into_targets: None,
                        bulk_collect: false,
                        into_table: None,
                        from: vec![],
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
                    }),
                };
                continue;
            }

            let right = self.parse_expr_with_precedence(if is_right_assoc { op_prec } else { op_prec + 1 })?;

            left = Expr::BinaryOp { left: Box::new(left), op: op_str, right: Box::new(right) };
        }

        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParserError> {
        if self.match_keyword(Keyword::PRIOR) {
            self.advance();
            let expr = self.parse_expr_with_precedence(15)?;
            return Ok(Expr::Prior(Box::new(expr)));
        }
        if self.match_keyword(Keyword::NOT) {
            self.advance();
            let expr = self.parse_expr_with_precedence(12)?;
            return Ok(Expr::UnaryOp { op: "NOT".to_string(), expr: Box::new(expr) });
        }
        if self.match_token(&Token::Minus) {
            self.advance();
            let expr = self.parse_expr_with_precedence(60)?;
            return Ok(Expr::UnaryOp { op: "-".to_string(), expr: Box::new(expr) });
        }
        if self.match_token(&Token::Plus) {
            self.advance();
            return self.parse_expr_with_precedence(60);
        }
        if self.match_token(&Token::At) {
            self.advance();
            let expr = self.parse_expr_with_precedence(60)?;
            return Ok(Expr::UnaryOp { op: "@".to_string(), expr: Box::new(expr) });
        }
        if let Some(op) = self.peek().as_op_str() {
            if matches!(op, "|/" | "||/" | "!!" | "?|" | "?-" | "?-|" | "?||" | "#" | "~" | "@@") {
                let op_str = op.to_string();
                self.advance();
                let expr = self.parse_expr_with_precedence(60)?;
                return Ok(Expr::UnaryOp { op: op_str, expr: Box::new(expr) });
            }
        }
        self.parse_primary_expr()
    }

    fn infix_operator(&self) -> Option<(u8, String, bool)> {
        match self.peek() {
            Token::Keyword(Keyword::OR) => Some((5, "OR".to_string(), false)),
            Token::Keyword(Keyword::AND) => Some((10, "AND".to_string(), false)),
            Token::Eq => Some((20, "=".to_string(), false)),
            Token::Lt => Some((20, "<".to_string(), false)),
            Token::Gt => Some((20, ">".to_string(), false)),
            Token::OpLe => Some((20, "<=".to_string(), false)),
            Token::OpNe => Some((20, "<>".to_string(), false)),
            Token::OpNe2 => Some((20, "!=".to_string(), false)),
            Token::OpGe => Some((20, ">=".to_string(), false)),
            Token::OpShiftL => Some((30, "<<".to_string(), false)),
            Token::OpShiftR => Some((30, ">>".to_string(), false)),
            Token::OpArrow => Some((60, "->".to_string(), false)),
            Token::OpJsonArrow => Some((60, "->>".to_string(), false)),
            Token::OpDblBang => Some((60, "!!".to_string(), true)),
            Token::OpConcat => Some((50, "||".to_string(), false)),
            Token::Op(op) => {
                let prec = match op.as_str() {
                    "<?>" => 20,
                    _ => 30,
                };
                Some((prec, op.clone(), false))
            }
            Token::Plus => Some((40, "+".to_string(), false)),
            Token::Minus => Some((40, "-".to_string(), false)),
            Token::Star => Some((50, "*".to_string(), false)),
            Token::Slash => Some((50, "/".to_string(), false)),
            Token::Percent => Some((50, "%".to_string(), false)),
            Token::Caret => Some((55, "^".to_string(), false)),
            Token::Typecast => Some((90, "::".to_string(), false)),
            _ => None,
        }
    }

    fn try_parse_cursor_attribute(&mut self) -> Option<crate::ast::CursorAttributeKind> {
        let attr = match self.peek() {
            Token::Ident(s) => match s.to_uppercase().as_str() {
                "NOTFOUND" => crate::ast::CursorAttributeKind::NotFound,
                "FOUND" => crate::ast::CursorAttributeKind::Found,
                "ISOPEN" => crate::ast::CursorAttributeKind::IsOpen,
                "ROWCOUNT" => crate::ast::CursorAttributeKind::RowCount,
                "BULK_EXCEPTIONS" => crate::ast::CursorAttributeKind::BulkExceptions,
                _ => return None,
            },
            _ => return None,
        };
        self.advance();
        Some(attr)
    }

    fn try_postfix_op(&mut self, left: &mut Expr) -> Result<bool, ParserError> {
        // AT TIME ZONE / AT LOCAL
        if self.match_keyword(Keyword::AT) {
            let saved_pos = self.pos;
            self.advance();
            if self.match_keyword(Keyword::TIME) {
                self.advance();
                self.expect_keyword(Keyword::ZONE)?;
                let zone = self.parse_expr_with_precedence(51)?;
                *left =
                    Expr::AtTimeZone { expr: Box::new(std::mem::replace(left, Expr::Default)), zone: Box::new(zone) };
                return Ok(true);
            } else if self.match_keyword(Keyword::LOCAL) {
                self.advance();
                *left = Expr::AtTimeZone {
                    expr: Box::new(std::mem::replace(left, Expr::Default)),
                    zone: Box::new(Expr::Literal(Literal::String("localtime".to_string()))),
                };
                return Ok(true);
            }
            // Not AT TIME ZONE / AT LOCAL — rewind
            self.pos = saved_pos;
        }
        match self.peek() {
            Token::Keyword(Keyword::IS) => {
                if let Some(next) = self.tokens.get(self.pos + 1) {
                    match &next.token {
                        Token::Keyword(Keyword::NULL_P) => {
                            self.advance();
                            self.advance();
                            *left =
                                Expr::IsNull { expr: Box::new(std::mem::replace(left, Expr::Default)), negated: false };
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::NOT) => {
                            if let Some(next2) = self.tokens.get(self.pos + 2) {
                                if matches!(&next2.token, Token::Keyword(Keyword::NULL_P)) {
                                    self.advance();
                                    self.advance();
                                    self.advance();
                                    *left = Expr::IsNull {
                                        expr: Box::new(std::mem::replace(left, Expr::Default)),
                                        negated: true,
                                    };
                                    return Ok(true);
                                }
                                if matches!(&next2.token, Token::Keyword(Keyword::TRUE_P)) {
                                    self.advance();
                                    self.advance();
                                    self.advance();
                                    *left = Expr::IsBoolean {
                                        expr: Box::new(std::mem::replace(left, Expr::Default)),
                                        value: true,
                                        negated: true,
                                    };
                                    return Ok(true);
                                }
                                if matches!(&next2.token, Token::Keyword(Keyword::FALSE_P)) {
                                    self.advance();
                                    self.advance();
                                    self.advance();
                                    *left = Expr::IsBoolean {
                                        expr: Box::new(std::mem::replace(left, Expr::Default)),
                                        value: false,
                                        negated: true,
                                    };
                                    return Ok(true);
                                }
                                if matches!(&next2.token, Token::Keyword(Keyword::DISTINCT)) {
                                    if let Some(next3) = self.tokens.get(self.pos + 3) {
                                        if matches!(&next3.token, Token::Keyword(Keyword::FROM)) {
                                            self.advance();
                                            self.advance();
                                            self.advance();
                                            self.advance();
                                            let right = self.parse_expr_with_precedence(5)?;
                                            let left_expr = std::mem::replace(left, Expr::Default);
                                            *left = Expr::BinaryOp {
                                                left: Box::new(left_expr),
                                                op: "IS NOT DISTINCT FROM".to_string(),
                                                right: Box::new(right),
                                            };
                                            return Ok(true);
                                        }
                                    }
                                }
                            }
                        }
                        Token::Keyword(Keyword::TRUE_P) => {
                            self.advance();
                            self.advance();
                            *left = Expr::IsBoolean {
                                expr: Box::new(std::mem::replace(left, Expr::Default)),
                                value: true,
                                negated: false,
                            };
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::FALSE_P) => {
                            self.advance();
                            self.advance();
                            *left = Expr::IsBoolean {
                                expr: Box::new(std::mem::replace(left, Expr::Default)),
                                value: false,
                                negated: false,
                            };
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::DISTINCT) => {
                            if let Some(next2) = self.tokens.get(self.pos + 2) {
                                if matches!(&next2.token, Token::Keyword(Keyword::FROM)) {
                                    self.advance();
                                    self.advance();
                                    self.advance();
                                    let right = self.parse_expr_with_precedence(5)?;
                                    let left_expr = std::mem::replace(left, Expr::Default);
                                    *left = Expr::BinaryOp {
                                        left: Box::new(left_expr),
                                        op: "IS DISTINCT FROM".to_string(),
                                        right: Box::new(right),
                                    };
                                    return Ok(true);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(false)
            }
            Token::Keyword(Keyword::ISNULL) => {
                self.advance();
                *left = Expr::IsNull { expr: Box::new(std::mem::replace(left, Expr::Default)), negated: false };
                Ok(true)
            }
            Token::Keyword(Keyword::NOTNULL) => {
                self.advance();
                *left = Expr::IsNull { expr: Box::new(std::mem::replace(left, Expr::Default)), negated: true };
                Ok(true)
            }
            Token::Keyword(Keyword::BETWEEN) => {
                self.advance();
                let low = self.parse_expr_with_precedence(40)?;
                self.expect_keyword(Keyword::AND)?;
                let high = self.parse_expr_with_precedence(40)?;
                *left = Expr::Between {
                    expr: Box::new(std::mem::replace(left, Expr::Default)),
                    low: Box::new(low),
                    high: Box::new(high),
                    negated: false,
                };
                Ok(true)
            }
            Token::Keyword(Keyword::NOT) => {
                if let Some(tws) = self.tokens.get(self.pos + 1) {
                    match &tws.token {
                        Token::Keyword(Keyword::BETWEEN) => {
                            self.advance();
                            self.advance();
                            let low = self.parse_expr_with_precedence(40)?;
                            self.expect_keyword(Keyword::AND)?;
                            let high = self.parse_expr_with_precedence(40)?;
                            *left = Expr::Between {
                                expr: Box::new(std::mem::replace(left, Expr::Default)),
                                low: Box::new(low),
                                high: Box::new(high),
                                negated: true,
                            };
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::IN_P) => {
                            self.advance();
                            self.advance();
                            *left = self.parse_in_expr(std::mem::replace(left, Expr::Default), true)?;
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::LIKE) => {
                            self.advance();
                            self.advance();
                            let pattern = self.parse_expr_with_precedence(11)?;
                            let escape = if self.match_keyword(Keyword::ESCAPE) {
                                self.advance();
                                Some(Box::new(self.parse_expr()?))
                            } else {
                                None
                            };
                            *left = Expr::Like {
                                expr: Box::new(std::mem::replace(left, Expr::Default)),
                                pattern: Box::new(pattern),
                                escape,
                                negated: true,
                                case_insensitive: false,
                            };
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::ILIKE) => {
                            self.advance();
                            self.advance();
                            let pattern = self.parse_expr_with_precedence(11)?;
                            let escape = if self.match_keyword(Keyword::ESCAPE) {
                                self.advance();
                                Some(Box::new(self.parse_expr()?))
                            } else {
                                None
                            };
                            *left = Expr::Like {
                                expr: Box::new(std::mem::replace(left, Expr::Default)),
                                pattern: Box::new(pattern),
                                escape,
                                negated: true,
                                case_insensitive: true,
                            };
                            return Ok(true);
                        }
                        Token::Keyword(Keyword::SIMILAR) => {
                            self.advance();
                            self.advance();
                            self.expect_keyword(Keyword::TO)?;
                            let pattern = self.parse_expr_with_precedence(11)?;
                            *left = Expr::BinaryOp {
                                left: Box::new(std::mem::replace(left, Expr::Default)),
                                op: "NOT SIMILAR TO".to_string(),
                                right: Box::new(pattern),
                            };
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
                Ok(false)
            }
            Token::Keyword(Keyword::IN_P) => {
                self.advance();
                *left = self.parse_in_expr(std::mem::replace(left, Expr::Default), false)?;
                Ok(true)
            }
            Token::Keyword(Keyword::LIKE) => {
                self.advance();
                let pattern = self.parse_expr_with_precedence(11)?;
                let escape = if self.match_keyword(Keyword::ESCAPE) {
                    self.advance();
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                *left = Expr::Like {
                    expr: Box::new(std::mem::replace(left, Expr::Default)),
                    pattern: Box::new(pattern),
                    escape,
                    negated: false,
                    case_insensitive: false,
                };
                Ok(true)
            }
            Token::Keyword(Keyword::ILIKE) => {
                self.advance();
                let pattern = self.parse_expr_with_precedence(11)?;
                let escape = if self.match_keyword(Keyword::ESCAPE) {
                    self.advance();
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                *left = Expr::Like {
                    expr: Box::new(std::mem::replace(left, Expr::Default)),
                    pattern: Box::new(pattern),
                    escape,
                    negated: false,
                    case_insensitive: true,
                };
                Ok(true)
            }
            Token::Keyword(Keyword::SIMILAR) => {
                self.advance();
                self.expect_keyword(Keyword::TO)?;
                let pattern = self.parse_expr_with_precedence(11)?;
                *left = Expr::BinaryOp {
                    left: Box::new(std::mem::replace(left, Expr::Default)),
                    op: "SIMILAR TO".to_string(),
                    right: Box::new(pattern),
                };
                Ok(true)
            }
            Token::LBracket => {
                self.advance();
                // Check if starts with : ([:upper] slice)
                let (lower, upper, is_slice) = if self.match_token(&Token::Colon) {
                    self.advance();
                    // [:upper]
                    let upper =
                        if self.match_token(&Token::RBracket) { None } else { Some(Box::new(self.parse_expr()?)) };
                    (None, upper, true)
                } else {
                    let first = self.parse_expr()?;
                    if self.match_token(&Token::Colon) {
                        self.advance();
                        // [lower:upper] or [lower:]
                        let upper =
                            if self.match_token(&Token::RBracket) { None } else { Some(Box::new(self.parse_expr()?)) };
                        (Some(Box::new(first)), upper, true)
                    } else {
                        // [expr]
                        (Some(Box::new(first)), None, false)
                    }
                };
                self.expect_token(&Token::RBracket)?;
                *left = Expr::Subscript {
                    object: Box::new(std::mem::replace(left, Expr::Default)),
                    lower,
                    upper,
                    is_slice,
                };
                Ok(true)
            }
            Token::LParen => {
                if let Some(next) = self.tokens.get(self.pos + 1) {
                    if matches!(&next.token, Token::Plus) {
                        if let Some(next2) = self.tokens.get(self.pos + 2) {
                            if matches!(&next2.token, Token::RParen) {
                                let loc = self.current_location();
                                self.advance();
                                self.advance();
                                self.advance();
                                self.add_error(ParserError::Warning {
                                    message: "Oracle-style outer join operator '(+)' is deprecated, use standard JOIN syntax instead".to_string(),
                                    location: loc,
                                    level: crate::linter::WarningLevel::Suggestion,
                                });
                                if let Expr::ColumnRef(name) = &*left {
                                    *left = Expr::ColumnRefOuterJoin(name.clone());
                                }
                                return Ok(true);
                            }
                        }
                    }
                }
                if matches!(left, Expr::CursorAttribute { .. }) {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect_token(&Token::RParen)?;
                    *left = Expr::Subscript {
                        object: Box::new(std::mem::replace(left, Expr::Default)),
                        lower: Some(Box::new(index)),
                        upper: None,
                        is_slice: false,
                    };
                    return Ok(true);
                }
                Ok(false)
            }
            Token::Op(op) if op == "!" => {
                self.advance();
                *left = Expr::UnaryOp { op: "!".to_string(), expr: Box::new(std::mem::replace(left, Expr::Default)) };
                Ok(true)
            }
            Token::Dot => {
                if matches!(left, Expr::Parenthesized(_))
                    || matches!(left, Expr::FunctionCall { .. })
                    || matches!(left, Expr::FieldAccess { .. })
                    || matches!(left, Expr::CursorAttribute { .. })
                {
                    self.advance();
                    let field = self.parse_identifier()?;
                    if self.match_token(&Token::LParen) {
                        self.advance();
                        let args = if self.match_token(&Token::RParen) {
                            vec![]
                        } else {
                            let mut args = vec![self.parse_expr()?];
                            while self.match_token(&Token::Comma) {
                                self.advance();
                                let (arg, _) = self.parse_maybe_named_arg()?;
                                args.push(arg);
                            }
                            args
                        };
                        self.expect_token(&Token::RParen)?;
                        let obj = std::mem::replace(left, Expr::Default);
                        let mut name_parts = expr_to_dotted_name(obj);
                        name_parts.push(field.into());
                        *left = Expr::FunctionCall {
                            name: name_parts,
                            args,
                            distinct: false,
                            over: None,
                            filter: None,
                            within_group: vec![],
                            separator: None,
                            default: None,
                            conversion_format: None,
                            agg_from: None,
                            builtin: None,
                        };
                    } else {
                        *left = Expr::FieldAccess { object: Box::new(std::mem::replace(left, Expr::Default)), field };
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            Token::Percent => {
                if !self.scope_stack.is_empty() {
                    self.advance();
                    if let Some(attr) = self.try_parse_cursor_attribute() {
                        *left = Expr::CursorAttribute {
                            cursor: Box::new(std::mem::replace(left, Expr::Default)),
                            attribute: attr,
                        };
                        return Ok(true);
                    }
                    self.pos -= 1;
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn parse_in_expr(&mut self, left: Expr, negated: bool) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        if self.match_keyword(Keyword::SELECT) || self.match_keyword(Keyword::WITH) {
            let subquery = self.parse_select_statement()?;
            self.expect_token(&Token::RParen)?;
            return Ok(Expr::InSubquery { expr: Box::new(left), subquery: Box::new(subquery), negated });
        }
        // Handle IN ((SELECT...) UNION (SELECT...)) — parenthesized subquery with set operations
        if self.match_token(&Token::LParen) {
            let save_pos = self.pos;
            let save_err_len = self.errors.len();
            if let Ok(subquery) = self.parse_select_statement() {
                if self.match_token(&Token::RParen) {
                    self.advance();
                    return Ok(Expr::InSubquery { expr: Box::new(left), subquery: Box::new(subquery), negated });
                }
            }
            self.pos = save_pos;
            self.errors.truncate(save_err_len);
        }
        let mut list = vec![self.parse_expr()?];
        while self.match_token(&Token::Comma) {
            self.advance();
            list.push(self.parse_expr()?);
        }
        self.expect_token(&Token::RParen)?;
        Ok(Expr::InList { expr: Box::new(left), list, negated })
    }

    pub(crate) fn parse_primary_expr(&mut self) -> Result<Expr, ParserError> {
        // Skip any optimizer hints that appear mid-expression (e.g., expr + /*+ hint */ more_expr)
        let _ = self.consume_hints();
        match self.peek().clone() {
            Token::Integer(n) => {
                self.advance();
                if self.match_token(&Token::Dot) {
                    if let Some(next) = self.tokens.get(self.pos + 1) {
                        if let Token::Integer(frac) = &next.token {
                            let frac = *frac;
                            self.advance();
                            self.advance();
                            let float_str = format!("{}.{}", n, frac);
                            return Ok(Expr::Literal(Literal::Float(float_str)));
                        }
                    }
                }
                if let Token::Ident(s) = self.peek() {
                    let lower = s.to_lowercase();
                    if lower.starts_with('e') && lower.len() > 1 && lower[1..].chars().all(|c| c.is_ascii_digit()) {
                        let exp_str = s.clone();
                        self.advance();
                        let float_str = format!("{}{}", n, exp_str);
                        return Ok(Expr::Literal(Literal::Float(float_str)));
                    }
                }
                Ok(Expr::Literal(Literal::Integer(n)))
            }
            Token::Float(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(s)))
            }
            Token::StringLiteral(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::String(s)))
            }
            Token::EscapeString(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::EscapeString(s)))
            }
            Token::BitString(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::BitString(s)))
            }
            Token::HexString(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::HexString(s)))
            }
            Token::NationalString(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::NationalString(s)))
            }
            Token::DollarString { tag, body } => {
                self.advance();
                Ok(Expr::Literal(Literal::DollarString { tag, body }))
            }
            Token::Keyword(Keyword::TRUE_P) => {
                self.advance();
                Ok(Expr::Literal(Literal::Boolean(true)))
            }
            Token::Keyword(Keyword::FALSE_P) => {
                self.advance();
                Ok(Expr::Literal(Literal::Boolean(false)))
            }
            Token::Keyword(Keyword::NULL_P) => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            Token::Keyword(Keyword::DEFAULT) => {
                self.advance();
                if !self.match_token(&Token::RParen) && !self.match_token(&Token::Comma) {
                    let val = self.parse_expr()?;
                    if self.match_keyword(Keyword::ON) || self.match_ident_str("on") {
                        self.advance();
                        if self.match_ident_str("CONVERSION") {
                            self.advance();
                        }
                        if self.match_ident_str("ERROR") {
                            self.advance();
                        }
                    }
                    Ok(val)
                } else {
                    Ok(Expr::Default)
                }
            }
            Token::Param(n) => {
                self.advance();
                Ok(Expr::Parameter(n))
            }
            Token::MyBatisParam(content) => {
                self.advance();
                Ok(Expr::MyBatisParam(content.clone()))
            }
            Token::MyBatisRawExpr(content) => {
                self.advance();
                Ok(Expr::MyBatisRawExpr(content.clone()))
            }
            Token::JdbcParam => {
                self.advance();
                Ok(Expr::JdbcParam)
            }
            Token::Keyword(Keyword::EXISTS) => {
                self.advance();
                self.expect_token(&Token::LParen)?;
                let subquery = self.parse_select_statement()?;
                self.expect_token(&Token::RParen)?;
                Ok(Expr::Exists(Box::new(subquery)))
            }
            Token::Keyword(Keyword::CASE) => self.parse_case_expr(),
            Token::LParen => {
                self.advance();
                if self.match_keyword(Keyword::SELECT) || self.match_keyword(Keyword::WITH) {
                    let subquery = self.parse_select_statement()?;
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::Subquery(Box::new(subquery)));
                }
                let expr = self.parse_expr()?;
                if self.match_token(&Token::Comma) {
                    let mut elems = vec![expr];
                    loop {
                        self.advance();
                        elems.push(self.parse_expr()?);
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::RowConstructor(elems));
                }
                self.expect_token(&Token::RParen)?;
                Ok(Expr::Parenthesized(Box::new(expr)))
            }
            Token::Keyword(Keyword::ARRAY) => {
                self.advance();
                if self.match_token(&Token::LParen) {
                    self.advance();
                    if self.match_keyword(Keyword::SELECT) || self.match_keyword(Keyword::WITH) {
                        let subquery = self.parse_select_statement()?;
                        self.expect_token(&Token::RParen)?;
                        return Ok(Expr::Subquery(Box::new(subquery)));
                    }
                    let mut elems = vec![self.parse_expr()?];
                    while self.match_token(&Token::Comma) {
                        self.advance();
                        elems.push(self.parse_expr()?);
                    }
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::Array(elems));
                } else if self.match_token(&Token::LBracket) {
                    self.advance();
                    if self.match_keyword(Keyword::SELECT) || self.match_keyword(Keyword::WITH) {
                        let subquery = self.parse_select_statement()?;
                        self.expect_token(&Token::RBracket)?;
                        return Ok(Expr::Subquery(Box::new(subquery)));
                    }
                    let mut elems = vec![self.parse_expr()?];
                    while self.match_token(&Token::Comma) {
                        self.advance();
                        elems.push(self.parse_expr()?);
                    }
                    self.expect_token(&Token::RBracket)?;
                    return Ok(Expr::Array(elems));
                }
                Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "'(' or '[' after ARRAY".to_string(),
                    got: format!("{:?}", self.peek()),
                })
            }
            Token::Ident(_) | Token::QuotedIdent(_) => {
                let name = self.parse_column_ref_or_qualified_star()?;
                Ok(name)
            }
            Token::Keyword(kw) => {
                // CAST(expr AS type) — must handle before generic keyword arm
                if kw == Keyword::CAST {
                    self.advance();
                    self.expect_token(&Token::LParen)?;
                    let expr = self.parse_expr()?;
                    if !self.match_keyword(Keyword::AS) {
                        return Err(ParserError::UnexpectedToken {
                            location: self.current_location(),
                            expected: "AS in CAST expression".to_string(),
                            got: format!("{:?}", self.peek()),
                        });
                    }
                    self.advance();
                    let type_name = self.parse_data_type()?;
                    let default = if self.match_keyword(Keyword::DEFAULT) || self.match_ident_str("default") {
                        self.advance();
                        let val = self.parse_expr()?;
                        if self.match_keyword(Keyword::ON) || self.match_ident_str("on") {
                            self.advance();
                            if self.match_ident_str("CONVERSION") {
                                self.advance();
                            }
                            if self.match_ident_str("ERROR") {
                                self.advance();
                            }
                        }
                        Some(Box::new(val))
                    } else {
                        None
                    };
                    let format = if default.is_some() && self.match_token(&Token::Comma) {
                        self.advance();
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::TypeCast { expr: Box::new(expr), type_name, default, format });
                }
                if self.match_ident_str("TREAT") {
                    self.advance();
                    self.expect_token(&Token::LParen)?;
                    let expr = self.parse_expr()?;
                    self.expect_keyword(Keyword::AS)?;
                    let type_name = self.parse_data_type()?;
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::Treat { expr: Box::new(expr), type_name });
                }
                if kw == Keyword::COLLATION
                    && self.tokens.get(self.pos + 1).is_some_and(|t| matches!(t.token, Token::Keyword(Keyword::FOR)))
                {
                    self.advance();
                    self.advance();
                    self.expect_token(&Token::LParen)?;
                    let expr = self.parse_expr()?;
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::CollationFor { expr: Box::new(expr) });
                }
                // PREDICT BY model_name (FEATURES col1, col2, ...) — openGauss ML prediction
                if kw == Keyword::PREDICT {
                    self.advance();
                    self.expect_keyword(Keyword::BY)?;
                    let model_name = self.parse_identifier()?;
                    self.expect_token(&Token::LParen)?;
                    self.expect_keyword(Keyword::FEATURES)?;
                    let mut features = vec![self.parse_expr()?];
                    while self.match_token(&Token::Comma) {
                        self.advance();
                        features.push(self.parse_expr()?);
                    }
                    self.expect_token(&Token::RParen)?;
                    return Ok(Expr::PredictBy { model_name, features });
                }
                if kw == Keyword::XMLELEMENT {
                    self.advance();
                    return self.parse_xml_element();
                }
                if kw == Keyword::XMLCONCAT {
                    self.advance();
                    return self.parse_xml_concat();
                }
                if kw == Keyword::XMLFOREST {
                    self.advance();
                    return self.parse_xml_forest();
                }
                if kw == Keyword::XMLPARSE {
                    self.advance();
                    return self.parse_xml_parse();
                }
                if kw == Keyword::XMLPI {
                    self.advance();
                    return self.parse_xml_pi();
                }
                if kw == Keyword::XMLROOT {
                    self.advance();
                    return self.parse_xml_root();
                }
                if kw == Keyword::XMLSERIALIZE {
                    self.advance();
                    return self.parse_xml_serialize();
                }
                // Type literal: DATE '...', TIME [WITH TIME ZONE] '...', TIMESTAMP [WITH TIME ZONE] '...'
                if matches!(kw, Keyword::DATE_P | Keyword::TIME | Keyword::TIMESTAMP) {
                    let type_name = kw.as_str().to_string();
                    let saved = self.pos;
                    self.advance();
                    let mut tz = None;
                    if self.match_keyword(Keyword::WITH) {
                        self.advance();
                        if self.match_keyword(Keyword::TIME) {
                            self.advance();
                            if self.match_keyword(Keyword::ZONE) {
                                self.advance();
                                tz = Some("WITH TIME ZONE");
                            }
                        }
                    } else if self.match_keyword(Keyword::WITHOUT) {
                        self.advance();
                        if self.match_keyword(Keyword::TIME) {
                            self.advance();
                            if self.match_keyword(Keyword::ZONE) {
                                self.advance();
                                tz = Some("WITHOUT TIME ZONE");
                            }
                        }
                    }
                    if let Token::StringLiteral(s) = self.peek().clone() {
                        self.advance();
                        let full_type = match tz {
                            Some(t) => format!("{} {}", type_name, t),
                            None => type_name,
                        };
                        return Ok(Expr::TypeCast {
                            expr: Box::new(Expr::Literal(Literal::String(s))),
                            type_name: DataType::Custom(vec![full_type.into()], Vec::new()),
                            default: None,
                            format: None,
                        });
                    }
                    // Not a type literal, backtrack
                    self.pos = saved;
                }
                if kw == Keyword::INTERVAL {
                    self.advance();
                    if let Token::StringLiteral(s) = self.peek().clone() {
                        self.advance();
                        let unit = self.peek_keyword();
                        if let Some(unit_kw) = unit {
                            if matches!(
                                unit_kw,
                                Keyword::DAY_P
                                    | Keyword::YEAR_P
                                    | Keyword::MONTH_P
                                    | Keyword::HOUR_P
                                    | Keyword::MINUTE_P
                                    | Keyword::SECOND_P
                                    | Keyword::DAY_HOUR_P
                                    | Keyword::DAY_MINUTE_P
                                    | Keyword::DAY_SECOND_P
                                    | Keyword::HOUR_MINUTE_P
                                    | Keyword::HOUR_SECOND_P
                                    | Keyword::MINUTE_SECOND_P
                                    | Keyword::YEAR_MONTH_P
                            ) {
                                let unit_name = unit_kw.as_str().to_string();
                                self.advance();
                                let builtin = crate::parser::function_registry::lookup_builtin_meta("interval");
                                return Ok(Expr::SpecialFunction {
                                    name: "interval".to_string(),
                                    args: vec![
                                        Expr::Literal(Literal::String(s)),
                                        Expr::ColumnRef(vec![unit_name.into()]),
                                    ],
                                    builtin,
                                });
                            }
                        }
                        return Ok(Expr::TypeCast {
                            expr: Box::new(Expr::Literal(Literal::String(s))),
                            type_name: DataType::Custom(vec!["interval".to_string().into()], Vec::new()),
                            default: None,
                            format: None,
                        });
                    }
                    if self.is_expr_start() {
                        let expr = self.parse_expr()?;
                        let unit = self.peek_keyword();
                        if let Some(unit_kw) = unit {
                            if matches!(
                                unit_kw,
                                Keyword::DAY_P
                                    | Keyword::YEAR_P
                                    | Keyword::MONTH_P
                                    | Keyword::HOUR_P
                                    | Keyword::MINUTE_P
                                    | Keyword::SECOND_P
                                    | Keyword::DAY_HOUR_P
                                    | Keyword::DAY_MINUTE_P
                                    | Keyword::DAY_SECOND_P
                                    | Keyword::HOUR_MINUTE_P
                                    | Keyword::HOUR_SECOND_P
                                    | Keyword::MINUTE_SECOND_P
                                    | Keyword::YEAR_MONTH_P
                            ) {
                                let unit_name = unit_kw.as_str().to_string();
                                self.advance();
                                let builtin = crate::parser::function_registry::lookup_builtin_meta("interval");
                                return Ok(Expr::SpecialFunction {
                                    name: "interval".to_string(),
                                    args: vec![expr, Expr::ColumnRef(vec![unit_name.into()])],
                                    builtin,
                                });
                            }
                        }
                        return Ok(expr);
                    }
                    return Ok(Expr::ColumnRef(vec!["interval".to_string().into()]));
                }
                if matches!(kw, Keyword::SYSDATE) {
                    self.advance();
                    return Ok(Expr::SysDate);
                }
                // Built-in expression keywords that are RESERVED but valid as expressions
                if matches!(
                    kw,
                    Keyword::ROWNUM
                        | Keyword::MAXVALUE
                        | Keyword::CURRENT_DATE
                        | Keyword::CURRENT_CATALOG
                        | Keyword::CURRENT_USER
                        | Keyword::CURRENT_ROLE
                        | Keyword::SESSION_USER
                        | Keyword::USER
                ) {
                    self.advance();
                    return Ok(Expr::ColumnRef(vec![kw.as_str().to_string().into()]));
                }
                if matches!(
                    kw,
                    Keyword::CURRENT_TIME | Keyword::CURRENT_TIMESTAMP | Keyword::LOCALTIME | Keyword::LOCALTIMESTAMP
                ) {
                    let name = kw.as_str().to_string();
                    self.advance();
                    if self.match_token(&Token::LParen) {
                        self.advance();
                        if self.match_token(&Token::RParen) {
                            self.advance();
                            let builtin = crate::parser::function_registry::lookup_builtin_meta(&name);
                            return Ok(Expr::SpecialFunction { name, args: vec![], builtin });
                        }
                        let precision = self.parse_expr()?;
                        self.expect_token(&Token::RParen)?;
                        let builtin = crate::parser::function_registry::lookup_builtin_meta(&name);
                        return Ok(Expr::SpecialFunction { name, args: vec![precision], builtin });
                    }
                    return Ok(Expr::ColumnRef(vec![name.into()]));
                }
                // CURRENT OF cursor_name — positioned UPDATE/DELETE
                if kw == Keyword::CURRENT_P {
                    self.advance();
                    if self.match_keyword(Keyword::OF) {
                        self.advance();
                        let cursor_name = self.parse_identifier()?;
                        return Ok(Expr::CurrentOf { cursor_name });
                    }
                    // Not "CURRENT OF" — fall through to treat CURRENT as column ref
                    return Ok(Expr::ColumnRef(vec!["CURRENT".to_string().into()]));
                }
                // Handle multi-word type names before string literals (e.g. double precision 'x')
                let saved = self.pos;
                let saved_err_len = self.errors.len();
                if let Ok(type_name) = self.try_parse_typecast_literal(kw) {
                    return Ok(type_name);
                }
                self.pos = saved;
                self.errors.truncate(saved_err_len);
                // Parse first identifier (keyword-as-identifier),
                // then check for qualified star (e.g. temp.*) before
                // continuing with dotted name resolution.
                let first = self.parse_ident()?;
                let name: ObjectName = if self.match_token(&Token::Dot) {
                    self.advance();
                    if self.match_token(&Token::Star) {
                        self.advance();
                        return Ok(Expr::QualifiedStar(first.value));
                    }
                    let mut parts = vec![first];
                    parts.push(self.parse_ident()?);
                    while self.match_token(&Token::Dot) {
                        self.advance();
                        parts.push(self.parse_ident()?);
                    }
                    parts
                } else {
                    vec![first]
                };
                if let Token::StringLiteral(s) = self.peek().clone() {
                    self.advance();
                    return Ok(Expr::TypeCast {
                        expr: Box::new(Expr::Literal(Literal::String(s))),
                        type_name: DataType::Custom(name, Vec::new()),
                        default: None,
                        format: None,
                    });
                }
                if self.match_token(&Token::LParen) {
                    if self.tokens.len() > self.pos + 2 {
                        let next = &self.tokens[self.pos + 1].token;
                        let next2 = &self.tokens[self.pos + 2].token;
                        if matches!(next, Token::Plus) && matches!(next2, Token::RParen) {
                            let loc = self.current_location();
                            self.advance();
                            self.advance();
                            self.advance();
                            self.add_error(ParserError::Warning {
                                message: "Oracle-style outer join operator '(+)' is deprecated, use standard JOIN syntax instead".to_string(),
                                location: loc,
                                level: crate::linter::WarningLevel::Suggestion,
                            });
                            return Ok(Expr::ColumnRefOuterJoin(name));
                        }
                    }
                    return self.parse_function_call(name);
                }
                if name.len() >= 2 {
                    let last = name.last().expect("name has at least 2 elements when len() >= 2").to_lowercase();
                    if last == "nextval" || last == "currval" {
                        let func = if last == "nextval" { SequenceFunc::Nextval } else { SequenceFunc::Currval };
                        let mut seq_parts = name.clone();
                        seq_parts.pop();
                        return Ok(Expr::SequenceValue { sequence: seq_parts, function: func });
                    }
                }
                // PL context: resolve single-part names against scope stack
                if !self.scope_stack.is_empty() && name.len() == 1 && self.is_var_declared(&name[0]) {
                    Ok(Expr::PlVariable(name))
                } else {
                    Ok(Expr::ColumnRef(name))
                }
            }
            Token::SetIdent(s) => {
                self.advance();
                Ok(Expr::ColumnRef(vec![s.into()]))
            }
            Token::Star => {
                self.advance();
                Ok(Expr::ColumnRef(vec!["*".to_string().into()]))
            }
            Token::LBracket => {
                self.advance();
                let mut elems = vec![self.parse_expr()?];
                while self.match_token(&Token::Comma) {
                    self.advance();
                    elems.push(self.parse_expr()?);
                }
                self.expect_token(&Token::RBracket)?;
                Ok(Expr::Array(elems))
            }
            _ => Err(ParserError::UnexpectedToken {
                location: self.current_location(),
                expected: "expression".to_string(),
                got: format!("{:?}", self.peek()),
            }),
        }
    }

    fn parse_column_ref_or_qualified_star(&mut self) -> Result<Expr, ParserError> {
        let mut first = self.parse_identifier()?;
        if self.match_token(&Token::At) {
            self.advance();
            let version = self.consume_any_identifier().or_else(|_| match self.peek().clone() {
                Token::Integer(n) => {
                    self.advance();
                    Ok(n.to_string())
                }
                Token::Float(f) => {
                    self.advance();
                    Ok(f)
                }
                other => Err(ParserError::UnexpectedToken {
                    location: self.current_location(),
                    expected: "version after @".to_string(),
                    got: format!("{:?}", other),
                }),
            })?;
            first = format!("{}@{}", first, version);
        }
        if self.match_token(&Token::Dot) {
            self.advance();
            if self.match_token(&Token::Star) {
                self.advance();
                return Ok(Expr::QualifiedStar(first));
            }
            let mut name: ObjectName = vec![first.into()];
            name.push(self.parse_ident()?);
            while self.match_token(&Token::Dot) {
                self.advance();
                name.push(self.parse_ident()?);
            }
            let obj_name = name;
            if obj_name.len() >= 2 {
                let last = obj_name.last().expect("obj_name has at least 2 elements when len() >= 2").to_lowercase();
                if last == "nextval" || last == "currval" {
                    let func = if last == "nextval" { SequenceFunc::Nextval } else { SequenceFunc::Currval };
                    let mut seq_parts = obj_name.clone();
                    seq_parts.pop();
                    return Ok(Expr::SequenceValue { sequence: seq_parts, function: func });
                }
            }
            if let Token::StringLiteral(s) = self.peek().clone() {
                self.advance();
                return Ok(Expr::TypeCast {
                    expr: Box::new(Expr::Literal(Literal::String(s))),
                    type_name: DataType::Custom(obj_name, Vec::new()),
                    default: None,
                    format: None,
                });
            }
            if self.match_token(&Token::LParen) {
                // Check for Oracle-style outer join (+): LParen at pos, Plus at pos+1, RParen at pos+2
                if self.tokens.len() > self.pos + 2 {
                    let next = &self.tokens[self.pos + 1].token;
                    let next2 = &self.tokens[self.pos + 2].token;
                    if matches!(next, Token::Plus) && matches!(next2, Token::RParen) {
                        let loc = self.current_location();
                        self.advance();
                        self.advance();
                        self.advance();
                        self.add_error(ParserError::Warning {
                            message:
                                "Oracle-style outer join operator '(+)' is deprecated, use standard JOIN syntax instead"
                                    .to_string(),
                            location: loc,
                            level: crate::linter::WarningLevel::Suggestion,
                        });
                        return Ok(Expr::ColumnRefOuterJoin(obj_name));
                    }
                }
                // Detect method-style calls: object.method(args) → method(object, args)
                // Applicable when the fully-qualified name is not a registered function
                // but the method name alone IS registered with min_args >= 1.
                if obj_name.len() >= 2 {
                    let full = obj_name.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>().join(".");
                    let method = obj_name.last().unwrap().to_lowercase();
                    let is_qualified_registered =
                        crate::parser::function_registry::lookup_builtin_meta_qualified(&full).is_some();
                    if !is_qualified_registered {
                        if let Some(meta) = crate::parser::function_registry::lookup_function(&method) {
                            if meta.min_args >= 1 {
                                let object_name: ObjectName = obj_name[..obj_name.len() - 1].to_vec();
                                let method_name: ObjectName = vec![obj_name.last().unwrap().clone()];
                                let object_expr = Expr::ColumnRef(object_name);

                                // Save error count to suppress false arg-count warnings
                                // from the inner call (method-style implicitly adds the
                                // object as the first argument).
                                let saved_error_count = self.errors.len();
                                let mut result = self.parse_function_call(method_name)?;
                                // Discard any arg-count warnings generated by the inner
                                // call — they used the wrong arg count (missing the
                                // implicit object argument).
                                self.errors.truncate(saved_error_count);

                                if let Expr::FunctionCall {
                                    ref mut name,
                                    ref mut args,
                                    ref mut builtin,
                                    ref mut over,
                                    ..
                                } = result
                                {
                                    // Prepend the object as first argument
                                    let mut new_args = vec![object_expr];
                                    new_args.append(args);
                                    *args = new_args;
                                    // Re-resolve builtin and re-validate with correct
                                    // arg count (object + paren args).
                                    let corrected_count = args.len();
                                    let has_over = over.is_some();
                                    *builtin = self.validate_func(name, corrected_count, false, has_over, false);
                                }
                                return Ok(result);
                            }
                        }
                    }
                }
                return self.parse_function_call(obj_name);
            }
            if !self.scope_stack.is_empty() && obj_name.len() == 1 && self.is_var_declared(&obj_name[0]) {
                return Ok(Expr::PlVariable(obj_name));
            }
            return Ok(Expr::ColumnRef(obj_name));
        }
        let name: ObjectName = vec![first.into()];
        if let Token::StringLiteral(s) = self.peek().clone() {
            self.advance();
            return Ok(Expr::TypeCast {
                expr: Box::new(Expr::Literal(Literal::String(s))),
                type_name: DataType::Custom(name, Vec::new()),
                default: None,
                format: None,
            });
        }
        if self.match_token(&Token::LParen) {
            // Check for Oracle-style outer join (+): LParen at pos, Plus at pos+1, RParen at pos+2
            if self.tokens.len() > self.pos + 2 {
                let next = &self.tokens[self.pos + 1].token;
                let next2 = &self.tokens[self.pos + 2].token;
                if matches!(next, Token::Plus) && matches!(next2, Token::RParen) {
                    let loc = self.current_location();
                    self.advance();
                    self.advance();
                    self.advance();
                    self.add_error(ParserError::Warning {
                        message:
                            "Oracle-style outer join operator '(+)' is deprecated, use standard JOIN syntax instead"
                                .to_string(),
                        location: loc,
                        level: crate::linter::WarningLevel::Suggestion,
                    });
                    return Ok(Expr::ColumnRefOuterJoin(name));
                }
            }
            return self.parse_function_call(name);
        }
        if !self.scope_stack.is_empty() && self.is_var_declared(&name[0]) {
            Ok(Expr::PlVariable(name))
        } else {
            Ok(Expr::ColumnRef(name))
        }
    }

    fn validate_function_args(&mut self, name: &ObjectName, args: &[Expr], distinct: bool, over: &Option<Box<Expr>>) {
        if let Some(last) = name.last() {
            let loc = self.current_location();
            let warnings = crate::parser::function_registry::validate_function_call(
                last,
                args.len(),
                distinct,
                over.is_some(),
                false,
                loc,
            );
            for w in warnings {
                self.add_error(w);
            }
        }
    }

    pub(crate) fn parse_maybe_named_arg(&mut self) -> Result<(Expr, bool), ParserError> {
        let has_variadic = self.try_consume_keyword(Keyword::VARIADIC);
        if matches!(self.peek(), Token::Ident(_)) || matches!(self.peek(), Token::Keyword(_)) {
            if let Some(tws) = self.tokens.get(self.pos + 1) {
                if matches!(tws.token, Token::ParamEquals) {
                    let _name = self.consume_any_identifier()?;
                    self.advance();
                    let expr = self.parse_expr()?;
                    return Ok((expr, has_variadic));
                }
            }
        }
        let first = self.parse_expr()?;
        if self.match_keyword(Keyword::AS) {
            self.advance();
            let _alias = self.parse_expr()?;
            Ok((first, has_variadic))
        } else if self.match_token(&Token::ParamEquals) {
            self.advance();
            let value = self.parse_expr()?;
            Ok((value, has_variadic))
        } else if self.match_keyword(Keyword::VALUE_P) {
            self.advance();
            let value = self.parse_expr()?;
            Ok((value, has_variadic))
        } else {
            Ok((first, has_variadic))
        }
    }

    /// Parse a function call after the opening `(` has been consumed.
    ///
    /// Certain functions with keyword-separated syntax (SUBSTRING, OVERLAY, POSITION,
    /// EXTRACT, TRIM, CONVERT) are routed to dedicated parsers that produce
    /// `Expr::SpecialFunction` instead of `Expr::FunctionCall`. See the
    /// [`SpecialFunction`](crate::ast::Expr::SpecialFunction) variant documentation
    /// for the complete list and rationale.
    fn parse_function_call(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;

        let lower_name = name.last().map(|s| s.to_lowercase()).unwrap_or_default();
        let is_qualified = name.len() > 1;
        if lower_name == "overlay" && !is_qualified {
            return self.parse_overlay_function(name);
        }
        if lower_name == "position" {
            return self.parse_position_function(name);
        }
        // `substring`/`substr` keyword syntax (FROM/FOR) → SpecialFunction;
        // comma syntax → FunctionCall. See parse_substring_function and #256.
        if lower_name == "substring" || lower_name == "substr" {
            return self.parse_substring_function(name);
        }
        if lower_name == "extract" {
            return self.parse_extract_function(name);
        }
        if lower_name == "trim" {
            return self.parse_trim_function(name);
        }
        if lower_name == "convert" {
            return self.parse_convert_function(name);
        }
        if lower_name == "group_concat" {
            return self.parse_group_concat_function(name);
        }

        if self.match_token(&Token::RParen) {
            self.advance();
            let filter = self.try_parse_filter()?;
            let within_group = self.try_parse_within_group()?;
            let over = self.try_parse_over_clause()?;
            let builtin = self.validate_func(&name, 0, false, over.is_some(), false);
            return Ok(Expr::FunctionCall {
                name,
                args: vec![],
                distinct: false,
                over,
                filter,
                within_group,
                separator: None,
                default: None,
                conversion_format: None,
                agg_from: None,
                builtin,
            });
        }
        let distinct = self.try_consume_keyword(Keyword::DISTINCT);
        if self.match_token(&Token::Star) {
            self.advance();
            self.expect_token(&Token::RParen)?;
            let filter = self.try_parse_filter()?;
            let within_group = self.try_parse_within_group()?;
            let over = self.try_parse_over_clause()?;
            let builtin = self.validate_func(&name, 1, distinct, over.is_some(), false);
            return Ok(Expr::FunctionCall {
                name,
                args: vec![Expr::ColumnRef(vec!["*".to_string().into()])],
                distinct,
                over,
                filter,
                within_group,
                separator: None,
                default: None,
                conversion_format: None,
                agg_from: None,
                builtin,
            });
        }
        let (first_arg, has_variadic) = self.parse_maybe_named_arg()?;
        let mut args = vec![first_arg];
        let default = if self.match_keyword(Keyword::DEFAULT) || self.match_ident_str("default") {
            self.advance();
            let val = self.parse_expr()?;
            if self.match_keyword(Keyword::ON) || self.match_ident_str("on") {
                self.advance();
                if self.match_ident_str("CONVERSION") {
                    self.advance();
                }
                if self.match_ident_str("ERROR") {
                    self.advance();
                }
            }
            Some(Box::new(val))
        } else {
            None
        };
        let conversion_format = if default.is_some() && self.match_token(&Token::Comma) {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let mut has_variadic = has_variadic;
        let mut agg_from: Option<Vec<crate::ast::TableRef>> = None;
        loop {
            // Check for FROM after last argument
            if agg_from.is_none() && self.match_keyword(Keyword::FROM) {
                self.advance();
                let from_tables = vec![self.parse_table_ref()?];
                if let Some(Expr::FunctionCall { ref within_group, .. }) = args.last() {
                    if !within_group.is_empty() {
                        // Implicit SELECT: wrap last arg in Subquery (#81)
                        let last_arg = args.pop().expect("args is non-empty when args.last() returned Some");
                        let select = SelectStatement {
                            hints: vec![],
                            with: None,
                            distinct: false,
                            distinct_on: vec![],
                            targets: vec![SelectTarget::Expr(last_arg, None)],
                            into_targets: None,
                            bulk_collect: false,
                            into_table: None,
                            from: from_tables,
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
                        };
                        args.push(Expr::Subquery(Box::new(select)));
                    } else {
                        agg_from = Some(from_tables);
                    }
                } else {
                    agg_from = Some(from_tables);
                }
                // FROM consumed; continue loop to check for more commas
                continue;
            }
            // Check for comma (more arguments)
            if self.match_token(&Token::Comma) {
                self.advance();
                let (arg, v) = self.parse_maybe_named_arg()?;
                has_variadic = has_variadic || v;
                args.push(arg);
            } else {
                break;
            }
        }
        if self.match_keyword(Keyword::PASSING) {
            self.advance();
            if self.match_keyword(Keyword::BY) {
                self.advance();
                let _ = self.try_consume_keyword(Keyword::REF);
                if self.match_ident_str("VALUE") {
                    self.advance();
                }
            }
            let (arg, v) = self.parse_maybe_named_arg()?;
            has_variadic = has_variadic || v;
            args.push(arg);
            while self.match_token(&Token::Comma) {
                self.advance();
                let (arg, v) = self.parse_maybe_named_arg()?;
                has_variadic = has_variadic || v;
                args.push(arg);
            }
        }
        let inner_order_by = if self.match_keyword(Keyword::ORDER) {
            self.advance();
            self.expect_keyword(Keyword::BY)?;
            let mut items = Vec::new();
            loop {
                let expr = self.parse_expr()?;
                let asc = match self.peek_keyword() {
                    Some(Keyword::ASC) => {
                        self.advance();
                        Some(true)
                    }
                    Some(Keyword::DESC) => {
                        self.advance();
                        Some(false)
                    }
                    _ => None,
                };
                items.push(OrderByItem { expr, asc, nulls_first: None, using: None });
                if !self.match_token(&Token::Comma) {
                    break;
                }
                self.advance();
            }
            items
        } else {
            vec![]
        };
        self.expect_token(&Token::RParen)?;
        let filter = self.try_parse_filter()?;
        let within_group = self.try_parse_within_group()?;
        let over = self.try_parse_over_clause()?;
        let builtin = self.validate_func(&name, args.len(), distinct, over.is_some(), has_variadic);
        let mut wg = inner_order_by;
        if !within_group.is_empty() {
            wg.extend(within_group);
        }
        Ok(Expr::FunctionCall {
            name,
            args,
            distinct,
            over,
            filter,
            within_group: wg,
            separator: None,
            default,
            conversion_format,
            agg_from,
            builtin,
        })
    }

    fn parse_convert_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let first = self.parse_expr()?;
        if self.match_keyword(Keyword::USING) {
            self.advance();
            let charset = self.parse_expr()?;
            self.expect_token(&Token::RParen)?;
            let builtin = crate::parser::function_registry::lookup_builtin_meta(
                &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
            );
            return Ok(Expr::SpecialFunction { name: name.join("."), args: vec![first, charset], builtin });
        }
        let mut args = vec![first];
        while self.match_token(&Token::Comma) {
            self.advance();
            args.push(self.parse_expr()?);
        }
        self.expect_token(&Token::RParen)?;
        let filter = self.try_parse_filter()?;
        let within_group = self.try_parse_within_group()?;
        let over = self.try_parse_over_clause()?;
        let builtin = self.validate_func(&name, args.len(), false, over.is_some(), false);
        Ok(Expr::FunctionCall {
            name,
            args,
            distinct: false,
            over,
            filter,
            within_group,
            separator: None,
            default: None,
            conversion_format: None,
            agg_from: None,
            builtin,
        })
    }

    fn parse_group_concat_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let distinct = self.try_consume_keyword(Keyword::DISTINCT);
        let mut args = vec![self.parse_expr()?];
        while self.match_token(&Token::Comma) {
            self.advance();
            args.push(self.parse_expr()?);
        }
        let separator = if self.match_keyword(Keyword::SEPARATOR_P) {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let inner_order_by = if self.match_keyword(Keyword::ORDER) {
            self.advance();
            self.expect_keyword(Keyword::BY)?;
            let mut items = Vec::new();
            loop {
                let expr = self.parse_expr()?;
                let asc = match self.peek_keyword() {
                    Some(Keyword::ASC) => {
                        self.advance();
                        Some(true)
                    }
                    Some(Keyword::DESC) => {
                        self.advance();
                        Some(false)
                    }
                    _ => None,
                };
                items.push(OrderByItem { expr, asc, nulls_first: None, using: None });
                if !self.match_token(&Token::Comma) {
                    break;
                }
                self.advance();
            }
            items
        } else {
            Vec::new()
        };
        self.expect_token(&Token::RParen)?;
        let filter = self.try_parse_filter()?;
        let within_group = self.try_parse_within_group()?;
        let combined_wg = if inner_order_by.is_empty() { within_group } else { inner_order_by };
        let over = self.try_parse_over_clause()?;
        let builtin = self.validate_func(&name, args.len(), distinct, over.is_some(), false);
        Ok(Expr::FunctionCall {
            name,
            args,
            distinct,
            over,
            filter,
            within_group: combined_wg,
            separator,
            default: None,
            conversion_format: None,
            agg_from: None,
            builtin,
        })
    }

    fn parse_overlay_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let arg1 = self.parse_expr()?;
        self.expect_keyword(Keyword::PLACING)?;
        let arg2 = self.parse_expr()?;
        self.expect_keyword(Keyword::FROM)?;
        let arg3 = self.parse_expr()?;
        let arg4 = if self.try_consume_keyword(Keyword::FOR) { Some(self.parse_expr()?) } else { None };
        self.expect_token(&Token::RParen)?;
        let mut args = vec![arg1, arg2, arg3];
        if let Some(a) = arg4 {
            args.push(a);
        }
        let builtin = crate::parser::function_registry::lookup_builtin_meta(
            &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
        );
        Ok(Expr::SpecialFunction { name: name.join("."), args, builtin })
    }

    fn parse_position_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let mut arg1 = self.parse_primary_expr()?;
        if self.match_token(&Token::Typecast) {
            self.advance();
            let type_name = self.parse_data_type()?;
            arg1 = Expr::TypeCast { expr: Box::new(arg1), type_name, default: None, format: None };
        }
        if self.match_keyword(Keyword::IN_P) {
            self.advance();
        } else if self.match_ident_str("IN") {
            self.advance();
        }
        let mut arg2 = self.parse_primary_expr()?;
        if self.match_token(&Token::Typecast) {
            self.advance();
            let type_name = self.parse_data_type()?;
            arg2 = Expr::TypeCast { expr: Box::new(arg2), type_name, default: None, format: None };
        }
        self.expect_token(&Token::RParen)?;
        let builtin = crate::parser::function_registry::lookup_builtin_meta(
            &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
        );
        Ok(Expr::SpecialFunction { name: name.join("."), args: vec![arg1, arg2], builtin })
    }

    fn parse_substring_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let arg1 = self.parse_expr()?;

        // Keyword-separated syntax (FROM / FOR) → SpecialFunction.
        if self.match_keyword(Keyword::FROM) {
            self.advance();
            let mut args = vec![arg1, self.parse_expr()?];
            if self.try_consume_keyword(Keyword::FOR) {
                args.push(self.parse_expr()?);
            }
            self.expect_token(&Token::RParen)?;
            let builtin = crate::parser::function_registry::lookup_builtin_meta(
                &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
            );
            return Ok(Expr::SpecialFunction { name: name.join("."), args, builtin });
        }
        if self.match_keyword(Keyword::FOR) {
            self.advance();
            let args = vec![arg1, self.parse_expr()?];
            self.expect_token(&Token::RParen)?;
            let builtin = crate::parser::function_registry::lookup_builtin_meta(
                &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
            );
            return Ok(Expr::SpecialFunction { name: name.join("."), args, builtin });
        }

        // Comma syntax (or bare single-arg form) → regular FunctionCall.
        // Mirrors parse_convert_function's comma path so that comma-syntax substr/substring
        // obtains builtin metadata, validate_func arg checking, and FILTER/OVER/WITHIN GROUP
        // support — consistent with how TRIM and CONVERT already split on syntax.
        let mut args = vec![arg1];
        while self.match_token(&Token::Comma) {
            self.advance();
            args.push(self.parse_expr()?);
        }
        self.expect_token(&Token::RParen)?;
        let filter = self.try_parse_filter()?;
        let within_group = self.try_parse_within_group()?;
        let over = self.try_parse_over_clause()?;
        let builtin = self.validate_func(&name, args.len(), false, over.is_some(), false);
        Ok(Expr::FunctionCall {
            name,
            args,
            distinct: false,
            over,
            filter,
            within_group,
            separator: None,
            default: None,
            conversion_format: None,
            agg_from: None,
            builtin,
        })
    }

    fn parse_extract_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let field = self.parse_identifier()?;
        self.expect_keyword(Keyword::FROM)?;
        let expr = self.parse_expr()?;
        self.expect_token(&Token::RParen)?;
        let builtin = crate::parser::function_registry::lookup_builtin_meta(
            &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
        );
        Ok(Expr::SpecialFunction {
            name: name.join("."),
            args: vec![Expr::ColumnRef(vec![field.into()]), expr],
            builtin,
        })
    }

    fn parse_trim_function(&mut self, name: ObjectName) -> Result<Expr, ParserError> {
        let direction = match self.peek_keyword() {
            Some(Keyword::LEADING) | Some(Keyword::TRAILING) | Some(Keyword::BOTH) => {
                let dir = self.peek_keyword().expect("peek_keyword matched in outer match arm").as_str().to_string();
                self.advance();
                Some(dir)
            }
            _ => None,
        };

        if let Some(dir) = direction {
            if self.match_keyword(Keyword::FROM) {
                // TRIM(direction FROM expr) — no explicit chars
                self.advance();
                let source = self.parse_expr()?;
                self.expect_token(&Token::RParen)?;
                let builtin = crate::parser::function_registry::lookup_builtin_meta(
                    &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
                );
                Ok(Expr::SpecialFunction {
                    name: name.join("."),
                    args: vec![Expr::ColumnRef(vec![dir.into()]), source],
                    builtin,
                })
            } else {
                // TRIM(direction chars FROM expr)
                let chars = self.parse_expr()?;
                self.expect_keyword(Keyword::FROM)?;
                let source = self.parse_expr()?;
                self.expect_token(&Token::RParen)?;
                let builtin = crate::parser::function_registry::lookup_builtin_meta(
                    &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
                );
                Ok(Expr::SpecialFunction {
                    name: name.join("."),
                    args: vec![Expr::ColumnRef(vec![dir.into()]), chars, source],
                    builtin,
                })
            }
        } else {
            // No direction keyword — could be TRIM(chars FROM expr) or TRIM(expr)
            let first = self.parse_expr()?;
            if self.match_keyword(Keyword::FROM) {
                // TRIM(chars FROM expr)
                self.advance();
                let source = self.parse_expr()?;
                self.expect_token(&Token::RParen)?;
                let builtin = crate::parser::function_registry::lookup_builtin_meta(
                    &name.last().map(|s| s.to_lowercase()).unwrap_or_default(),
                );
                Ok(Expr::SpecialFunction { name: name.join("."), args: vec![first, source], builtin })
            } else {
                // Regular function call: TRIM(expr [, ...])
                let mut args = vec![first];
                while self.match_token(&Token::Comma) {
                    self.advance();
                    let (arg, _) = self.parse_maybe_named_arg()?;
                    args.push(arg);
                }
                self.expect_token(&Token::RParen)?;
                let filter = self.try_parse_filter()?;
                let within_group = self.try_parse_within_group()?;
                let over = self.try_parse_over_clause()?;
                let builtin = self.validate_func(&name, args.len(), false, over.is_some(), false);
                Ok(Expr::FunctionCall {
                    name,
                    args,
                    distinct: false,
                    over,
                    filter,
                    within_group,
                    separator: None,
                    default: None,
                    conversion_format: None,
                    agg_from: None,
                    builtin,
                })
            }
        }
    }

    fn try_parse_filter(&mut self) -> Result<Option<Box<Expr>>, ParserError> {
        if self.match_ident_str("FILTER") {
            self.advance();
            self.expect_token(&Token::LParen)?;
            self.expect_keyword(Keyword::WHERE)?;
            let expr = self.parse_expr()?;
            self.expect_token(&Token::RParen)?;
            Ok(Some(Box::new(expr)))
        } else {
            Ok(None)
        }
    }

    fn try_parse_within_group(&mut self) -> Result<Vec<OrderByItem>, ParserError> {
        if self.match_keyword(Keyword::WITHIN) {
            self.advance();
            self.expect_keyword(Keyword::GROUP_P)?;
            self.expect_token(&Token::LParen)?;
            self.expect_keyword(Keyword::ORDER)?;
            self.expect_keyword(Keyword::BY)?;
            let mut items = Vec::new();
            loop {
                let expr = self.parse_expr()?;
                let asc = match self.peek_keyword() {
                    Some(Keyword::ASC) => {
                        self.advance();
                        Some(true)
                    }
                    Some(Keyword::DESC) => {
                        self.advance();
                        Some(false)
                    }
                    _ => None,
                };
                let nulls_first = if self.try_consume_keyword(Keyword::NULLS_P) {
                    if self.try_consume_keyword(Keyword::FIRST_P) {
                        Some(true)
                    } else {
                        self.expect_keyword(Keyword::LAST_P)?;
                        Some(false)
                    }
                } else {
                    None
                };
                items.push(OrderByItem { expr, asc, nulls_first, using: None });
                if !self.match_token(&Token::Comma) {
                    break;
                }
                self.advance();
            }
            self.expect_token(&Token::RParen)?;
            Ok(items)
        } else {
            Ok(Vec::new())
        }
    }

    /// Try to parse OVER clause after a function call.
    /// Returns None if the next token is not OVER.
    fn try_parse_over_clause(&mut self) -> Result<Option<WindowSpec>, ParserError> {
        if !self.match_keyword(Keyword::OVER) {
            return Ok(None);
        }
        self.advance();

        if self.match_token(&Token::LParen) {
            // OVER (window_specification)
            self.advance();
            let spec = self.parse_window_specification()?;
            self.expect_token(&Token::RParen)?;
            Ok(Some(spec))
        } else {
            // OVER window_name (identifier)
            let name = self.parse_identifier()?;
            Ok(Some(WindowSpec { partition_by: vec![], order_by: vec![], frame: None, window_name: Some(name) }))
        }
    }

    /// Parse the body of a window specification (inside parens).
    /// Grammar: [existing_window_name] [PARTITION BY expr_list] [ORDER BY sort_clause] [frame_clause]
    pub(crate) fn parse_window_specification(&mut self) -> Result<WindowSpec, ParserError> {
        // Try to parse existing window name (an identifier that is NOT PARTITION, ORDER, ROWS, RANGE)
        let window_name = self.try_parse_window_name();

        // PARTITION BY expr_list
        let partition_by = if self.match_keyword(Keyword::PARTITION) {
            self.advance();
            self.expect_keyword(Keyword::BY)?;
            let mut items = vec![self.parse_expr()?];
            while self.match_token(&Token::Comma) {
                self.advance();
                items.push(self.parse_expr()?);
            }
            items
        } else {
            vec![]
        };

        // ORDER BY sort_clause
        let order_by = if self.match_keyword(Keyword::ORDER) {
            self.advance();
            self.expect_keyword(Keyword::BY)?;
            let mut items = Vec::new();
            loop {
                let expr = self.parse_expr()?;
                let asc = match self.peek_keyword() {
                    Some(Keyword::ASC) => {
                        self.advance();
                        Some(true)
                    }
                    Some(Keyword::DESC) => {
                        self.advance();
                        Some(false)
                    }
                    _ => None,
                };
                let nulls_first = if self.match_keyword(Keyword::NULLS_P) {
                    self.advance();
                    if self.match_keyword(Keyword::FIRST_P) {
                        self.advance();
                        Some(true)
                    } else {
                        self.expect_keyword(Keyword::LAST_P)?;
                        Some(false)
                    }
                } else {
                    None
                };
                items.push(OrderByItem { expr, asc, nulls_first, using: None });
                if !self.match_token(&Token::Comma) {
                    break;
                }
                self.advance();
            }
            items
        } else {
            vec![]
        };

        // Frame clause: ROWS|RANGE frame_extent
        let frame = if self.match_keyword(Keyword::ROWS)
            || self.match_keyword(Keyword::RANGE)
            || self.match_keyword(Keyword::GROUPS)
        {
            let mode = if self.match_keyword(Keyword::ROWS) {
                self.advance();
                WindowFrameMode::Rows
            } else if self.match_keyword(Keyword::RANGE) {
                self.advance();
                WindowFrameMode::Range
            } else {
                self.advance();
                WindowFrameMode::Groups
            };
            let (start, end) = self.parse_frame_extent()?;
            Some(WindowFrame { mode, start, end })
        } else {
            None
        };

        Ok(WindowSpec { partition_by, order_by, frame, window_name })
    }

    /// Try to parse an existing window name (identifier).
    /// Returns None if the next token looks like PARTITION, ORDER, ROWS, RANGE, or closing paren.
    fn try_parse_window_name(&mut self) -> Option<String> {
        match self.peek_keyword() {
            Some(Keyword::PARTITION) | Some(Keyword::ORDER) => None,
            _ => {
                match self.peek() {
                    Token::Ident(_) | Token::QuotedIdent(_) => {
                        // This is a window name if it's followed by something other than
                        // just more identifiers (i.e., PARTITION/ORDER follows, or RParen)
                        // Simple heuristic: take it as a window name
                        let name = match self.peek().clone() {
                            Token::Ident(s) => s,
                            Token::QuotedIdent(s) => s,
                            _ => unreachable!(),
                        };
                        self.advance();
                        Some(name)
                    }
                    _ => None,
                }
            }
        }
    }

    /// Parse frame_extent: BETWEEN frame_bound AND frame_bound | frame_bound
    fn parse_frame_extent(&mut self) -> Result<(Option<WindowFrameBound>, Option<WindowFrameBound>), ParserError> {
        if self.match_keyword(Keyword::BETWEEN) {
            self.advance();
            let start = self.parse_frame_bound()?;
            self.expect_keyword(Keyword::AND)?;
            let end = self.parse_frame_bound()?;
            Ok((Some(start), Some(end)))
        } else {
            let start = self.parse_frame_bound()?;
            Ok((Some(start), None))
        }
    }

    /// Parse a single frame bound:
    ///   UNBOUNDED PRECEDING | UNBOUNDED FOLLOWING | CURRENT ROW
    ///   n PRECEDING | n FOLLOWING
    fn parse_frame_bound(&mut self) -> Result<WindowFrameBound, ParserError> {
        if self.match_keyword(Keyword::UNBOUNDED) {
            self.advance();
            if self.match_keyword(Keyword::PRECEDING) {
                self.advance();
                Ok(WindowFrameBound { direction: WindowFrameDirection::UnboundedPreceding })
            } else {
                self.expect_keyword(Keyword::FOLLOWING)?;
                Ok(WindowFrameBound { direction: WindowFrameDirection::UnboundedFollowing })
            }
        } else if self.match_keyword(Keyword::CURRENT_P) {
            self.advance();
            self.expect_keyword(Keyword::ROW)?;
            Ok(WindowFrameBound { direction: WindowFrameDirection::CurrentRow })
        } else {
            // numeric offset PRECEDING | FOLLOWING
            let offset = match self.peek().clone() {
                Token::Integer(n) => {
                    self.advance();
                    n
                }
                _ => 0,
            };
            if self.match_keyword(Keyword::PRECEDING) {
                self.advance();
                Ok(WindowFrameBound { direction: WindowFrameDirection::Preceding(offset) })
            } else {
                self.expect_keyword(Keyword::FOLLOWING)?;
                Ok(WindowFrameBound { direction: WindowFrameDirection::Following(offset) })
            }
        }
    }

    fn parse_case_expr(&mut self) -> Result<Expr, ParserError> {
        self.expect_keyword(Keyword::CASE)?;
        let operand = if !self.match_keyword(Keyword::WHEN) { Some(Box::new(self.parse_expr()?)) } else { None };
        let mut whens = Vec::new();
        while self.match_keyword(Keyword::WHEN) {
            self.advance();
            let condition = self.parse_expr()?;
            self.expect_keyword(Keyword::THEN)?;
            let result = self.parse_expr()?;
            whens.push(WhenClause { condition, result });
        }
        let else_expr = if self.match_keyword(Keyword::ELSE) {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        self.expect_keyword(Keyword::END_P)?;
        Ok(Expr::Case { operand, whens, else_expr })
    }

    fn parse_xml_element(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;

        let entity_escaping = if self.try_consume_keyword(Keyword::ENTITYESCAPING) {
            Some(true)
        } else if self.try_consume_keyword(Keyword::NOENTITYESCAPING) {
            Some(false)
        } else {
            None
        };

        let (evalname, name) = if self.try_consume_keyword(Keyword::EVALNAME) {
            let expr = self.parse_expr()?;
            (Some(Box::new(expr)), None)
        } else {
            let _ = self.try_consume_keyword(Keyword::NAME_P);
            (None, Some(self.parse_identifier()?))
        };

        let mut attributes: Option<XmlAttributes> = None;
        let mut content: Vec<XmlContent> = Vec::new();

        while self.match_token(&Token::Comma) {
            self.advance();

            if self.match_keyword(Keyword::XMLATTRIBUTES) && attributes.is_none() {
                self.advance();
                attributes = Some(self.parse_xml_attributes_inner()?);
                continue;
            }

            let expr = self.parse_expr()?;
            let alias = self.parse_optional_alias()?;
            content.push(XmlContent { expr, alias });
        }

        self.expect_token(&Token::RParen)?;

        Ok(Expr::XmlElement { entity_escaping, evalname, name, attributes, content })
    }

    fn parse_xml_attributes_inner(&mut self) -> Result<XmlAttributes, ParserError> {
        self.expect_token(&Token::LParen)?;

        let entity_escaping = if self.try_consume_keyword(Keyword::ENTITYESCAPING) {
            Some(true)
        } else if self.try_consume_keyword(Keyword::NOENTITYESCAPING) {
            Some(false)
        } else {
            None
        };

        let mut items = Vec::new();
        loop {
            let expr = self.parse_expr()?;
            let name = self.parse_optional_alias()?;
            items.push(XmlAttribute { value: expr, name });
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }

        self.expect_token(&Token::RParen)?;
        Ok(XmlAttributes { entity_escaping, items })
    }

    fn parse_xml_concat(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        let mut args = vec![self.parse_expr()?];
        while self.match_token(&Token::Comma) {
            self.advance();
            args.push(self.parse_expr()?);
        }
        self.expect_token(&Token::RParen)?;
        Ok(Expr::XmlConcat(args))
    }

    fn parse_xml_forest(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        let mut items = Vec::new();
        loop {
            let expr = self.parse_expr()?;
            let alias = self.parse_optional_alias()?;
            items.push(XmlContent { expr, alias });
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }
        self.expect_token(&Token::RParen)?;
        Ok(Expr::XmlForest(items))
    }

    fn parse_xml_parse(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        let option = if self.match_keyword(Keyword::DOCUMENT_P) {
            self.advance();
            XmlOption::Document
        } else {
            self.expect_keyword(Keyword::CONTENT_P)?;
            XmlOption::Content
        };
        let expr = self.parse_expr()?;
        let wellformed = self.try_consume_keyword(Keyword::WELLFORMED);
        self.expect_token(&Token::RParen)?;
        Ok(Expr::XmlParse { option, expr: Box::new(expr), wellformed })
    }

    fn parse_xml_pi(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        let _ = self.try_consume_keyword(Keyword::NAME_P);
        let name = self.parse_identifier()?;
        let content = if self.match_token(&Token::Comma) {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        self.expect_token(&Token::RParen)?;
        Ok(Expr::XmlPi { name: Some(name), content })
    }

    fn parse_xml_root(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        let expr = self.parse_expr()?;
        self.expect_token(&Token::Comma)?;
        self.expect_keyword(Keyword::VERSION_P)?;
        let version = if self.match_keyword(Keyword::NO) {
            self.advance();
            if self.try_consume_keyword(Keyword::VALUE_P) {
                None
            } else {
                Some(Box::new(Expr::ColumnRef(vec!["no".to_string().into()])))
            }
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        let standalone = if self.match_token(&Token::Comma) {
            self.advance();
            self.expect_keyword(Keyword::STANDALONE_P)?;
            if self.try_consume_keyword(Keyword::YES_P) {
                Some(Some(true))
            } else if self.match_keyword(Keyword::NO) {
                self.advance();
                if self.try_consume_keyword(Keyword::VALUE_P) {
                    Some(None)
                } else {
                    Some(Some(false))
                }
            } else {
                None
            }
        } else {
            None
        };
        self.expect_token(&Token::RParen)?;
        Ok(Expr::XmlRoot { expr: Box::new(expr), version, standalone })
    }

    fn parse_xml_serialize(&mut self) -> Result<Expr, ParserError> {
        self.expect_token(&Token::LParen)?;
        let option = if self.match_keyword(Keyword::DOCUMENT_P) {
            self.advance();
            XmlOption::Document
        } else {
            self.expect_keyword(Keyword::CONTENT_P)?;
            XmlOption::Content
        };
        let expr = self.parse_expr()?;
        self.expect_keyword(Keyword::AS)?;
        let type_name = self.parse_data_type()?;
        self.expect_token(&Token::RParen)?;
        Ok(Expr::XmlSerialize { option, expr: Box::new(expr), type_name })
    }

    fn try_parse_typecast_literal(&mut self, first_kw: Keyword) -> Result<Expr, ParserError> {
        let type_name = self.parse_data_type()?;
        if let Token::StringLiteral(s) = self.peek().clone() {
            self.advance();
            return Ok(Expr::TypeCast {
                expr: Box::new(Expr::Literal(Literal::String(s))),
                type_name,
                default: None,
                format: None,
            });
        }
        Err(ParserError::UnexpectedToken {
            location: self.current_location(),
            expected: format!("string literal after type {}", first_kw.as_str()),
            got: format!("{:?}", self.peek()),
        })
    }
}

fn expr_to_dotted_name(expr: crate::ast::Expr) -> Vec<crate::ast::Ident> {
    match expr {
        crate::ast::Expr::ColumnRef(name) => name,
        crate::ast::Expr::PlVariable(name) => name,
        crate::ast::Expr::FunctionCall { name, .. } => name,
        crate::ast::Expr::FieldAccess { object, field } => {
            let mut parts = expr_to_dotted_name(*object);
            parts.push(field.into());
            parts
        }
        _ => vec![],
    }
}
