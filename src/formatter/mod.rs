// A3 exception: ast is a pure data module; this file references 1300+ AST types.
use crate::ast::*;

pub(crate) mod plpgsql;

pub struct SqlFormatter {
    indent: usize,
    uppercase_keywords: bool,
    pretty_print: bool,
}

impl SqlFormatter {
    /// Creates a new SQL formatter with default settings (uppercase keywords).
    pub fn new() -> Self {
        Self { indent: 0, uppercase_keywords: true, pretty_print: false }
    }

    /// Control whether SQL keywords are uppercased in the output.
    pub fn uppercase_keywords(mut self, yes: bool) -> Self {
        self.uppercase_keywords = yes;
        self
    }

    /// Enable multiline/pretty-print output for [`format_statement`](Self::format_statement).
    ///
    /// When enabled, line breaks are inserted before major SQL keywords
    /// (`FROM`, `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, etc.)
    /// with proper indentation and parenthesis-depth awareness (subqueries
    /// are left intact).
    pub fn pretty_print(mut self, yes: bool) -> Self {
        self.pretty_print = yes;
        self
    }

    /// Set the base indentation level (number of 2-space units) for
    /// pretty-printed output.
    pub fn indent(mut self, n: usize) -> Self {
        self.indent = n;
        self
    }

    /// Formats an AST statement back into SQL text.
    ///
    /// When [`pretty_print(true)`](Self::pretty_print) is enabled, the
    /// output is reformatted with line breaks before major keywords and
    /// proper indentation. Otherwise (default), the output is a single-line
    /// normalized SQL string.
    pub fn format_statement(&self, stmt: &Statement) -> String {
        if self.pretty_print {
            if let Some(pretty) = self.format_statement_pretty(stmt) {
                return pretty;
            }
        }
        self.format_statement_flat(stmt)
    }

    /// Render a single expression as normalized SQL text (no statement wrapper).
    pub fn render_expr(&self, expr: &Expr) -> String {
        self.format_expr(expr)
    }

    /// Structured pretty-print for SELECT statements. Returns None for
    /// non-SELECT statements (caller falls back to flat+multiline).
    fn format_statement_pretty(&self, stmt: &Statement) -> Option<String> {
        match stmt {
            Statement::Select(s) => Some(self.format_select_pretty(s, self.indent)),
            _ => {
                let flat = self.format_statement_flat(stmt);
                Some(self.multiline_format_sql(&flat, self.indent))
            }
        }
    }

    /// Flat (single-line) formatting — used internally and by the PL/pgSQL
    /// formatter which applies its own indentation.
    pub(crate) fn format_statement_flat(&self, stmt: &Statement) -> String {
        match stmt {
            Statement::Select(s) => self.format_select(s),
            Statement::Insert(s) => self.format_insert(s),
            Statement::InsertAll(s) => self.format_insert_all(s),
            Statement::InsertFirst(s) => self.format_insert_first(s),
            Statement::Update(s) => self.format_update(s),
            Statement::Delete(s) => self.format_delete(s),
            Statement::Merge(s) => self.format_merge(s),
            Statement::CreateTable(s) => self.format_create_table(s),
            Statement::AlterTable(s) => self.format_alter_table(s),
            Statement::AlterTablespace(s) => self.format_alter_tablespace(s),
            Statement::Drop(s) => self.format_drop(s),
            Statement::Truncate(s) => self.format_truncate(s),
            Statement::CreateIndex(s) => self.format_create_index(s),
            Statement::CreateGlobalIndex(s) => self.format_create_global_index(s),
            Statement::CreateSchema(s) => self.format_create_schema(s),
            Statement::CreateDatabase(s) => self.format_create_database(s),
            Statement::CreateDatabaseLink(s) => self.format_create_database_link(s),
            Statement::CreateTablespace(s) => self.format_create_tablespace(s),
            Statement::CreateView(s) => self.format_create_view(s),
            Statement::CreateSequence(s) => self.format_create_sequence(s),
            Statement::Transaction(s) => self.format_transaction(s),
            Statement::Copy(s) => self.format_copy(s),
            Statement::Explain(s) => self.format_explain(s),
            Statement::VariableSet(s) => self.format_variable_set(s),
            Statement::VariableShow(s) => self.format_variable_show(s),
            Statement::VariableReset(s) => self.format_variable_reset(s),
            Statement::Discard(s) => self.format_discard(s),
            Statement::Call(s) => self.format_call(s),
            Statement::CreateFdw(s) => self.format_create_fdw(s),
            Statement::CreateForeignServer(s) => self.format_create_foreign_server(s),
            Statement::CreateForeignTable(s) => self.format_create_foreign_table(s),
            Statement::CreatePublication(s) => self.format_create_publication(s),
            Statement::CreateSubscription(s) => self.format_create_subscription(s),
            Statement::CreateNode(s) => self.format_create_node(s),
            Statement::CreateNodeGroup(s) => self.format_create_node_group(s),
            Statement::CreateResourcePool(s) => self.format_create_resource_pool(s),
            Statement::CreateWorkloadGroup(s) => self.format_create_workload_group(s),
            Statement::CreateAuditPolicy(s) => self.format_create_audit_policy(s),
            Statement::CreateMaskingPolicy(s) => self.format_create_masking_policy(s),
            Statement::CreateRlsPolicy(s) => self.format_create_rls_policy(s),
            Statement::CreatePolicyLabel(s) => self.format_create_policy_label(s),
            Statement::AlterPolicyLabel(s) => self.format_alter_policy_label(s),
            Statement::AlterMaskingPolicy(s) => self.format_alter_masking_policy(s),
            Statement::Empty => String::new(),
            Statement::Checkpoint => self.kw("CHECKPOINT"),
            Statement::Grant(s) => self.format_grant(s),
            Statement::Revoke(s) => self.format_revoke(s),
            Statement::Vacuum(s) => self.format_vacuum(s),
            Statement::Analyze(s) => self.format_analyze(s),
            Statement::Do(s) => self.format_do(s),
            Statement::Prepare(s) => self.format_prepare(s),
            Statement::Execute(s) => self.format_execute(s),
            Statement::Deallocate(s) => self.format_deallocate(s),
            Statement::Comment(s) => self.format_comment(s),
            Statement::Lock(s) => self.format_lock(s),
            Statement::DeclareCursor(s) => self.format_declare_cursor(s),
            Statement::ClosePortal(s) => self.format_close_portal(s),
            Statement::Fetch(s) => self.format_fetch(s),
            Statement::Cluster(s) => self.format_cluster(s),
            Statement::Reindex(s) => self.format_reindex(s),
            Statement::Listen(s) => self.format_listen(s),
            Statement::Notify(s) => self.format_notify(s),
            Statement::Unlisten(s) => self.format_unlisten(s),
            Statement::Rule(s) => self.format_rule(s),
            Statement::CreateTrigger(s) => self.format_create_trigger(s),
            Statement::CreateMaterializedView(s) => self.format_create_materialized_view(s),
            Statement::RefreshMaterializedView(s) => self.format_refresh_matview(s),
            Statement::AlterDatabase(s) => self.format_alter_database(s),
            Statement::AlterSchema(s) => self.format_alter_schema(s),
            Statement::AlterSequence(s) => self.format_alter_sequence(s),
            Statement::AlterFunction(s) => self.format_alter_function(s),
            Statement::AlterProcedure(s) => self.format_alter_function_action(&s.action),
            Statement::AlterRole(s) => self.format_alter_role(s),
            Statement::AlterUser(s) => self.format_alter_user(s),
            Statement::AlterGlobalConfig(s) => self.format_alter_global_config(s),
            Statement::CreateFunction(s) => self.format_create_function(s),
            Statement::CreateProcedure(s) => self.format_create_procedure(s),
            Statement::CreatePackage(s) => self.format_create_package(s),
            Statement::CreatePackageBody(s) => self.format_create_package_body(s),
            Statement::CreateExtension(s) => self.format_create_extension(s),
            Statement::CreateRole(s) => self.format_create_role(s),
            Statement::CreateUser(s) => self.format_create_user(s),
            Statement::CreateGroup(s) => self.format_create_group(s),
            Statement::CreateCast(s) => self.format_create_cast(s),
            Statement::CreateDomain(s) => self.format_create_domain(s),
            Statement::GrantRole(s) => self.format_grant_role(s),
            Statement::RevokeRole(s) => self.format_revoke_role(s),
            Statement::AlterGroup(s) => self.format_alter_group(s),
            Statement::AlterCompositeType(s) => self.format_alter_composite_type(s),
            Statement::AlterView(s) => self.format_alter_view(s),
            Statement::AlterTrigger(s) => self.format_alter_trigger(s),
            Statement::AlterExtension(s) => self.format_alter_extension(s),
            Statement::AnonyBlock(s) => self.format_anon_block(s),
            Statement::CreateTableAs(s) => self.format_create_table_as(s),
            Statement::CreateAggregate(s) => self.format_create_aggregate(s),
            Statement::CreateOperator(s) => self.format_create_operator(s),
            Statement::AlterDefaultPrivileges(s) => self.format_alter_default_privileges(s),
            Statement::CreateUserMapping(s) => self.format_create_user_mapping(s),
            Statement::AlterUserMapping(s) => self.format_alter_user_mapping(s),
            Statement::DropUserMapping(s) => self.format_drop_user_mapping(s),
            Statement::Shutdown(s) => self.format_shutdown(s),
            Statement::Barrier(s) => self.format_barrier(s),
            Statement::Purge(s) => self.format_purge(s),
            Statement::TimeCapsule(s) => self.format_timecapsule(s),
            Statement::Snapshot(s) => self.format_snapshot(s),
            Statement::Shrink(s) => self.format_shrink(s),
            Statement::Verify(s) => self.format_verify(s),
            Statement::CleanConn(s) => self.format_clean_conn(s),
            Statement::Compile(s) => self.format_compile(s),
            Statement::SecLabel(s) => self.format_sec_label(s),
            Statement::CreateConversion(s) => self.format_create_conversion(s),
            Statement::CreateSynonym(s) => self.format_create_synonym(s),
            Statement::CreateModel(s) => self.format_create_model(s),
            Statement::CreateAm(s) => self.format_create_am(s),
            Statement::CreateDirectory(s) => self.format_create_directory(s),
            Statement::CreateDataSource(s) => self.format_create_data_source(s),
            Statement::CreateEvent(s) => self.format_create_event(s),
            Statement::CreateOpClass(s) => self.format_create_opclass(s),
            Statement::CreateOpFamily(s) => self.format_create_opfamily(s),
            Statement::CreateContQuery(s) => self.format_create_contquery(s),
            Statement::CreateStream(s) => self.format_create_stream(s),
            Statement::CreateKey(s) => self.format_create_key(s),
            Statement::AlterForeignTable(s) => self.format_alter_foreign_table(s),
            Statement::AlterForeignServer(s) => self.format_alter_foreign_server(s),
            Statement::AlterFdw(s) => self.format_alter_fdw(s),
            Statement::AlterPublication(s) => self.format_alter_publication(s),
            Statement::AlterSubscription(s) => self.format_alter_subscription(s),
            Statement::AlterNode(s) => self.format_alter_node(s),
            Statement::AlterNodeGroup(s) => self.format_alter_node_group(s),
            Statement::AlterWorkloadGroup(s) => self.format_alter_workload_group(s),
            Statement::AlterAuditPolicy(s) => self.format_alter_audit_policy(s),
            Statement::AlterRlsPolicy(s) => self.format_alter_rls_policy(s),
            Statement::AlterDataSource(s) => self.format_alter_data_source(s),
            Statement::AlterEvent(s) => self.format_alter_event(s),
            Statement::AlterOpFamily(s) => self.format_alter_opfamily(s),
            Statement::AlterMaterializedView(s) => self.format_alter_materialized_view(s),
            Statement::Abort => self.kw("ABORT"),
            Statement::Values(s) => self.format_values(s),
            Statement::ExecuteDirect(s) => self.format_execute_direct(s),
            Statement::CreateLanguage(s) => self.format_create_language(s),
            Statement::CreateWeakPasswordDictionaryWithValues(s) => {
                let mut r = self.kw("CREATE WEAK PASSWORD DICTIONARY");
                if !s.values.is_empty() {
                    r.push_str(&format!(
                        " WITH VALUES ({})",
                        s.values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ")
                    ));
                }
                r
            }
            Statement::PredictBy(s) => {
                let mut r = format!("{} {}", self.kw("PREDICT"), self.kw("BY"));
                r.push_str(&format!(" {}", s.model));
                r.push_str(&format!(" {} ({})", self.kw("FEATURES"), s.features.join(", ")));
                if let Some(ref u) = s.using_clause {
                    r.push_str(&format!(" {} {}", self.kw("USING"), u));
                }
                r
            }
            Statement::Replace(s) => {
                let mut r = self.kw("REPLACE");
                r.push_str(&format!(" {}", self.format_insert(s)));
                r
            }
            Statement::Move(s) => self.format_move(s),
            Statement::LockBuckets(s) => {
                format!("{} {}", self.kw("LOCK BUCKETS"), s.raw_rest)
            }
            Statement::MarkBuckets(s) => {
                format!("{} {}", self.kw("MARK BUCKETS"), s.raw_rest)
            }
            Statement::SetSessionAuthorization(s) => {
                if s.is_default {
                    self.kw("SET SESSION AUTHORIZATION DEFAULT").to_string()
                } else if let Some(ref u) = s.user {
                    format!("{} {}", self.kw("SET SESSION AUTHORIZATION"), u)
                } else {
                    self.kw("SET SESSION AUTHORIZATION").to_string()
                }
            }
            Statement::CreateAppWorkloadGroupMapping(s) => {
                format!("{} {} {}", self.kw("CREATE APP WORKLOAD GROUP MAPPING"), s.name, s.raw_rest)
            }
            Statement::DropAppWorkloadGroupMapping(s) => {
                format!("{} {}", self.kw("DROP APP WORKLOAD GROUP MAPPING"), s.name)
            }
            Statement::CreateTextSearchConfig(s) => {
                format!(
                    "{} {} {}",
                    self.kw("CREATE TEXT SEARCH CONFIGURATION"),
                    self.format_object_name(&s.name),
                    s.raw_rest
                )
            }
            Statement::CreateTextSearchDict(s) => {
                let opts: Vec<String> = s.options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
                format!(
                    "{} {} ({})",
                    self.kw("CREATE TEXT SEARCH DICTIONARY"),
                    self.format_object_name(&s.name),
                    opts.join(", ")
                )
            }
            Statement::AlterTextSearchConfigFull(s) => {
                format!(
                    "{} {} {}",
                    self.kw("ALTER TEXT SEARCH CONFIGURATION"),
                    self.format_object_name(&s.name),
                    s.raw_rest
                )
            }
            Statement::AlterTextSearchDictFull(s) => {
                let opts: Vec<String> = s.options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
                format!(
                    "{} {} ({})",
                    self.kw("ALTER TEXT SEARCH DICTIONARY"),
                    self.format_object_name(&s.name),
                    opts.join(", ")
                )
            }
            Statement::ExpdpDatabase(s) => {
                format!("{} {}", self.kw("EXPDP DATABASE"), s.raw_rest)
            }
            Statement::ExpdpTable(s) => {
                format!("{} {}", self.kw("EXPDP TABLE"), s.raw_rest)
            }
            Statement::ImpdpDatabase(s) => {
                format!("{} {}", self.kw("IMPDP DATABASE"), s.raw_rest)
            }
            Statement::ImpdpTable(s) => {
                format!("{} {}", self.kw("IMPDP TABLE"), s.raw_rest)
            }
            Statement::ReassignOwned(s) => {
                format!("{} {} {} {}", self.kw("REASSIGN OWNED BY"), s.old_role, self.kw("TO"), s.new_role)
            }
            Statement::AlterDomain(s) => {
                let mut r = format!("{} {}", self.kw("ALTER DOMAIN"), self.format_object_name(&s.name));
                match &s.action {
                    AlterDomainAction::SetDefault { expr } => {
                        r.push_str(&format!(" {} {}", self.kw("SET DEFAULT"), expr))
                    }
                    AlterDomainAction::DropDefault => r.push_str(&format!(" {}", self.kw("DROP DEFAULT"))),
                    AlterDomainAction::SetNotNull => r.push_str(&format!(" {}", self.kw("SET NOT NULL"))),
                    AlterDomainAction::DropNotNull => r.push_str(&format!(" {}", self.kw("DROP NOT NULL"))),
                    AlterDomainAction::AddConstraint { name, check_expr } => {
                        r.push_str(&format!(" {}", self.kw("ADD")));
                        if let Some(n) = name {
                            r.push_str(&format!(" {} {}", self.kw("CONSTRAINT"), n));
                        }
                        r.push_str(&format!(" {} {}", self.kw("CHECK"), check_expr));
                    }
                    AlterDomainAction::DropConstraint { name, cascade } => {
                        r.push_str(&format!(
                            " {} {} {}",
                            self.kw("DROP CONSTRAINT"),
                            name,
                            if *cascade { self.kw("CASCADE") } else { String::new() }
                        ));
                    }
                    AlterDomainAction::OwnerTo { new_owner } => {
                        r.push_str(&format!(" {} {}", self.kw("OWNER TO"), new_owner))
                    }
                    AlterDomainAction::RenameTo { new_name } => {
                        r.push_str(&format!(" {} {}", self.kw("RENAME TO"), new_name))
                    }
                    AlterDomainAction::ValidateConstraint { name } => {
                        r.push_str(&format!(" {} {}", self.kw("VALIDATE CONSTRAINT"), name))
                    }
                }
                r
            }
            _ => self.format_stub(stmt),
        }
    }

    fn kw(&self, s: &str) -> String {
        if self.uppercase_keywords {
            s.to_uppercase()
        } else {
            s.to_lowercase()
        }
    }

    fn format_stub(&self, stmt: &Statement) -> String {
        let name = match stmt {
            Statement::AlterDomain(_) => "ALTER DOMAIN",
            Statement::AlterResourcePool(_) => "ALTER RESOURCE POOL",
            Statement::GetDiag(_) => "GET DIAGNOSTICS",
            Statement::ShowEvent(_) => "SHOW EVENT",
            Statement::RemovePackage(_) => "REMOVE PACKAGE",
            Statement::CreateWeakPasswordDictionary => "CREATE WEAK PASSWORD DICTIONARY",
            Statement::DropWeakPasswordDictionary => "DROP WEAK PASSWORD DICTIONARY",
            Statement::CreatePolicyLabel(_) => "CREATE POLICY LABEL",
            Statement::AlterPolicyLabel(_) => "ALTER POLICY LABEL",
            Statement::DropPolicyLabel(_) => "DROP POLICY LABEL",
            _ => "UNKNOWN",
        };
        format!("/* stub: {} */", name)
    }

    fn indent_lines(&self, text: &str, indent: usize) -> String {
        if indent == 0 {
            return text.to_string();
        }
        let pad = Self::pad(indent);
        text.split('\n')
            .enumerate()
            .map(|(i, line)| if i == 0 { line.to_string() } else { format!("{}{}", pad, line) })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn multiline_format_sql(&self, flat: &str, indent: usize) -> String {
        // Break points: (pattern, is_conditional_clause).
        // JOIN variants are listed before bare " JOIN " so the overlap
        // dedup below keeps the longer match and discards the inner one.
        let break_before: [(&str, bool); 21] = [
            (" FROM ", false),
            (" LEFT JOIN ", false),
            (" RIGHT JOIN ", false),
            (" INNER JOIN ", false),
            (" FULL JOIN ", false),
            (" CROSS JOIN ", false),
            (" JOIN ", false),
            (" WHERE ", true),
            (" GROUP BY ", false),
            (" HAVING ", true),
            (" ORDER BY ", false),
            (" LIMIT ", false),
            (" OFFSET ", false),
            (" CONNECT BY ", false),
            (" START WITH ", false),
            (" UNION ALL ", false),
            (" UNION ", false),
            (" INTERSECT ALL ", false),
            (" INTERSECT ", false),
            (" EXCEPT ALL ", false),
            (" EXCEPT ", false),
        ];

        let depths = Self::compute_paren_depths(flat);

        let mut positions: Vec<(usize, usize, bool, i32)> = Vec::new();
        for (pattern, is_cond) in &break_before {
            let mut search_from = 0;
            while let Some(pos) = flat[search_from..].find(pattern) {
                let match_start = search_from + pos;
                let kw_start = match_start + 1;
                let kw_end = kw_start + pattern.trim().len();
                let paren_depth = depths.get(kw_start).copied().unwrap_or(0);
                positions.push((kw_start, kw_end, *is_cond, paren_depth));
                search_from = match_start + pattern.len();
                if search_from >= flat.len() {
                    break;
                }
            }
        }
        positions.sort_by_key(|(start, _, _, _)| *start);
        // Deduplicate: remove keywords whose range falls inside a longer
        // keyword at the same position (e.g. "JOIN" inside "LEFT JOIN").
        positions.dedup_by(|next, prev| next.0 >= prev.0 && next.0 < prev.1);

        if positions.is_empty() {
            return flat.to_string();
        }

        let mut result = String::new();

        for (i, (start, kw_end, is_cond, depth)) in positions.iter().enumerate() {
            if i == 0 && *start > 0 {
                result.push_str(flat[..*start].trim());
            }

            if !result.is_empty() {
                result.push('\n');
                // Indent proportionally to subquery nesting depth.
                let line_pad = Self::pad(indent + *depth as usize);
                result.push_str(&line_pad);
            }

            let kw_and_content_end = if i + 1 < positions.len() { positions[i + 1].0 } else { flat.len() };

            let kw_text = flat[*start..*kw_end].trim();
            let content = flat[*kw_end..kw_and_content_end].trim();

            if *is_cond && !content.is_empty() {
                let cond_cont_pad = format!("{}{}", Self::pad(indent + *depth as usize), Self::pad(1));
                result.push_str(kw_text);
                result.push(' ');
                result.push_str(&Self::break_logical_ops(content, &cond_cont_pad));
            } else {
                result.push_str(kw_text);
                if !content.is_empty() {
                    result.push(' ');
                    result.push_str(content);
                }
            }
        }

        if result.is_empty() {
            flat.to_string()
        } else {
            result
        }
    }

    /// Compute parenthesis nesting depth at every byte position of `s`.
    ///
    /// Returns a vector where `depths[i]` is the paren depth *after*
    /// processing byte `i-1` (i.e. the depth at position `i`).
    /// Works on raw bytes because `(` and `)` are single-byte ASCII.
    fn compute_paren_depths(s: &str) -> Vec<i32> {
        let bytes = s.as_bytes();
        let mut depths = Vec::with_capacity(bytes.len() + 1);
        let mut d = 0i32;
        depths.push(d);
        for &b in bytes {
            match b {
                b'(' => d += 1,
                b')' => d -= 1,
                _ => {}
            }
            depths.push(d);
        }
        depths
    }

    fn break_logical_ops(condition: &str, pad: &str) -> String {
        let cont_pad = format!("{}{}", pad, Self::pad(1));
        let mut result = String::new();
        let mut depth: i32 = 0;
        let chars = condition.chars().peekable();
        let mut current_token = String::new();
        let mut tokens: Vec<(String, i32)> = Vec::new();

        for ch in chars {
            match ch {
                '(' => {
                    if !current_token.is_empty() {
                        tokens.push((current_token.clone(), depth));
                        current_token.clear();
                    }
                    depth += 1;
                    tokens.push(("(".to_string(), depth));
                }
                ')' => {
                    if !current_token.is_empty() {
                        tokens.push((current_token.clone(), depth));
                        current_token.clear();
                    }
                    depth -= 1;
                    tokens.push((")".to_string(), depth));
                }
                ' ' | '\t' => {
                    if !current_token.is_empty() {
                        tokens.push((current_token.clone(), depth));
                        current_token.clear();
                    }
                }
                _ => {
                    current_token.push(ch);
                }
            }
        }
        if !current_token.is_empty() {
            tokens.push((current_token, depth));
        }

        let mut i = 0;
        while i < tokens.len() {
            let (ref tok, d) = tokens[i];
            if d == 0 && tok == "AND" {
                let is_between_and = Self::is_between_and(&tokens, i);
                if is_between_and {
                    if !result.ends_with(' ') {
                        result.push(' ');
                    }
                    result.push_str(tok);
                    result.push(' ');
                } else {
                    result = result.trim_end().to_string();
                    result.push('\n');
                    result.push_str(&cont_pad);
                    result.push_str(tok);
                    result.push(' ');
                }
            } else if d == 0 && tok == "OR" {
                result = result.trim_end().to_string();
                result.push('\n');
                result.push_str(&cont_pad);
                result.push_str(tok);
                result.push(' ');
            } else {
                if !result.is_empty() && !result.ends_with(' ') && !result.ends_with('(') && tok != ")" {
                    result.push(' ');
                }
                result.push_str(tok);
            }
            i += 1;
        }

        result
    }

    fn is_between_and(tokens: &[(String, i32)], and_idx: usize) -> bool {
        let mut j = and_idx as i32 - 1;
        let mut depth = 0i32;
        while j >= 0 {
            let (ref tok, _) = tokens[j as usize];
            if tok == ")" {
                depth += 1;
            } else if tok == "(" {
                depth -= 1;
            } else if depth == 0 && tok.to_uppercase() == "BETWEEN" {
                return true;
            } else if depth == 0 && tok == "AND" {
                return false;
            }
            j -= 1;
        }
        false
    }

    fn format_select_pretty(&self, stmt: &SelectStatement, indent_level: usize) -> String {
        let pad = Self::pad(indent_level);
        let mut lines: Vec<String> = Vec::new();

        if let Some(with) = &stmt.with {
            lines.push(format!("{}{}", pad, self.format_with(with)));
        }

        let mut select_kw = self.kw("SELECT").to_string();
        if let Some(hints) = self.format_hints(&stmt.hints) {
            select_kw.push(' ');
            select_kw.push_str(&hints);
        }
        if stmt.distinct {
            if stmt.distinct_on.is_empty() {
                select_kw.push(' ');
                select_kw.push_str(&self.kw("DISTINCT"));
            } else {
                let cols: Vec<String> = stmt.distinct_on.iter().map(|e| self.format_expr(e)).collect();
                select_kw.push(' ');
                select_kw.push_str(&self.kw("DISTINCT"));
                select_kw.push_str(" ON (");
                select_kw.push_str(&cols.join(", "));
                select_kw.push(')');
            }
        }

        let targets: Vec<String> =
            stmt.targets.iter().map(|t| self.format_select_target_pretty(t, indent_level + 1)).collect();
        if targets.len() <= 1 {
            lines.push(format!("{}{} {}", pad, select_kw, targets.first().map(|s| s.as_str()).unwrap_or("")));
        } else {
            lines.push(format!("{}{}", pad, select_kw));
            for (i, t) in targets.iter().enumerate() {
                let comma = if i + 1 < targets.len() { "," } else { "" };
                if t.contains('\n') {
                    lines.push(format!("{}  {}{}", pad, t.trim_start_matches(' '), comma));
                } else {
                    lines.push(format!("{}  {}{}", pad, t, comma));
                }
            }
        }

        if let Some(into_targets) = &stmt.into_targets {
            let prefix = if stmt.bulk_collect {
                format!("{} {} {}", self.kw("BULK"), self.kw("COLLECT"), self.kw("INTO"))
            } else {
                self.kw("INTO").to_string()
            };
            lines.push(format!("{}{} {}", pad, prefix, self.format_select_targets(into_targets)));
        }

        if let Some(into_table) = &stmt.into_table {
            let mut into_parts = vec![self.kw("INTO")];
            if into_table.unlogged {
                into_parts.push(self.kw("UNLOGGED"));
            }
            into_parts.push(self.kw("TABLE"));
            into_parts.push(self.format_object_name(&into_table.table_name));
            lines.push(format!("{}{}", pad, into_parts.join(" ")));
        }

        if !stmt.from.is_empty() {
            lines.push(format!(
                "{}{} {}",
                pad,
                self.kw("FROM"),
                self.format_table_refs_pretty(&stmt.from, indent_level)
            ));
        }

        if let Some(where_clause) = &stmt.where_clause {
            let where_str = self.format_expr_pretty(where_clause, indent_level + 1);
            lines.push(format!("{}{} {}", pad, self.kw("WHERE"), where_str));
        }

        if let Some(cb) = &stmt.connect_by {
            if let Some(sw) = &cb.start_with {
                lines.push(format!("{}{} {}", pad, self.kw("START WITH"), self.format_expr(sw)));
            }
            let nocycle = if cb.nocycle { " NOCYCLE" } else { "" };
            lines.push(format!("{}{}{} {}", pad, self.kw("CONNECT BY"), nocycle, self.format_expr(&cb.condition)));
        }

        if !stmt.group_by.is_empty() {
            let group_str =
                stmt.group_by.iter().map(|item| self.format_group_by_item(item)).collect::<Vec<_>>().join(", ");
            lines.push(format!("{}{} {}", pad, self.kw("GROUP BY"), group_str));
        }

        if let Some(having) = &stmt.having {
            lines.push(format!("{}{} {}", pad, self.kw("HAVING"), self.format_expr(having)));
        }

        let mut result = lines.join("\n");

        if let Some(set_op) = &stmt.set_operation {
            let (op_name, all, right) = match set_op {
                SetOperation::Union { all, right } => ("UNION", all, right),
                SetOperation::Intersect { all, right } => ("INTERSECT", all, right),
                SetOperation::Except { all, right } => ("EXCEPT", all, right),
            };
            let mut op_str = self.kw(op_name);
            if *all {
                op_str.push_str(" ALL");
            }
            result = format!("{}\n{}{}\n{}", result, pad, op_str, self.format_select_pretty(right, indent_level));
        }

        if !stmt.order_by.is_empty() {
            result.push_str(&format!("\n{}{} {}", pad, self.kw("ORDER BY"), self.format_order_by(&stmt.order_by)));
        }

        if !stmt.window_clause.is_empty() {
            let windows: Vec<String> = stmt
                .window_clause
                .iter()
                .map(|w| {
                    format!(
                        "{} {} ({})",
                        self.quote_identifier(&w.name),
                        self.kw("AS"),
                        self.format_window_spec(&w.spec)
                    )
                })
                .collect();
            result.push_str(&format!("\n{}{} {}", pad, self.kw("WINDOW"), windows.join(", ")));
        }

        if let Some(limit) = &stmt.limit {
            result.push_str(&format!("\n{}{} {}", pad, self.kw("LIMIT"), self.format_expr(limit)));
        }

        if let Some(offset) = &stmt.offset {
            result.push_str(&format!("\n{}{} {}", pad, self.kw("OFFSET"), self.format_expr(offset)));
        }

        result
    }

    fn format_table_refs_pretty(&self, refs: &[TableRef], indent_level: usize) -> String {
        let pad = Self::pad(indent_level);
        let mut parts: Vec<String> = Vec::new();
        for (i, r) in refs.iter().enumerate() {
            if i > 0 {
                parts.push(format!("{}{}", pad, self.format_table_ref_pretty(r, indent_level)));
            } else {
                parts.push(self.format_table_ref_pretty(r, indent_level));
            }
        }
        parts.join(",\n")
    }

    fn format_table_ref_pretty(&self, r: &TableRef, indent_level: usize) -> String {
        let pad = Self::pad(indent_level);
        match r {
            TableRef::Subquery { query, alias, lateral } => {
                let sub_pretty = self.format_select_pretty(query, indent_level + 1);
                let lat = if *lateral { format!("{} ", self.kw("LATERAL")) } else { String::new() };
                let alias_str = match alias {
                    Some(a) => format!(" AS {}", self.format_ident(a)),
                    None => String::new(),
                };
                format!("{}(\n{}\n{}){}", lat, sub_pretty, pad, alias_str)
            }
            TableRef::Join { left, right, join_type, condition, natural, using_columns } => {
                let left_str = self.format_table_ref_pretty(left, indent_level);
                let join_kw = match join_type {
                    JoinType::Inner => self.kw("INNER JOIN"),
                    JoinType::Left => self.kw("LEFT JOIN"),
                    JoinType::Right => self.kw("RIGHT JOIN"),
                    JoinType::Full => self.kw("FULL JOIN"),
                    JoinType::Cross => self.kw("CROSS JOIN"),
                };
                let natural_kw = if *natural { format!("{} ", self.kw("NATURAL")) } else { String::new() };
                let right_str = self.format_table_ref_pretty(right, indent_level);
                let mut result = format!("{}\n{}{}{} {}", left_str, pad, natural_kw, join_kw, right_str);
                if let Some(cond) = condition {
                    result.push_str(&format!("\n{}  {} {}", pad, self.kw("ON"), self.format_expr(cond)));
                } else if !using_columns.is_empty() {
                    result.push_str(&format!("\n{}  {} ({})", pad, self.kw("USING"), using_columns.join(", ")));
                }
                result
            }
            _ => self.format_table_ref(r),
        }
    }

    fn format_select_target_pretty(&self, target: &SelectTarget, indent_level: usize) -> String {
        match target {
            SelectTarget::Expr(expr, alias) => {
                let expr_str = self.format_expr_pretty_in_select(expr, indent_level);
                if let Some(a) = alias {
                    format!("{} AS {}", expr_str, self.format_ident(a))
                } else {
                    expr_str
                }
            }
            SelectTarget::Star(alias) => match alias {
                Some(a) => format!("{}.*", self.format_ident(a)),
                None => "*".to_string(),
            },
        }
    }

    fn format_expr_pretty_in_select(&self, expr: &Expr, indent_level: usize) -> String {
        match expr {
            Expr::Subquery(subquery) => {
                let close_pad = Self::pad(indent_level);
                let sub_pretty = self.format_select_pretty(subquery, indent_level + 1);
                format!("(\n{}\n{})", sub_pretty, close_pad)
            }
            Expr::Exists(subquery) => {
                let close_pad = Self::pad(indent_level);
                let sub_pretty = self.format_select_pretty(subquery, indent_level + 1);
                format!("{} (\n{}\n{})", self.kw("EXISTS"), sub_pretty, close_pad)
            }
            Expr::Case { operand, whens, else_expr } => {
                self.format_case_pretty(operand, whens, else_expr, indent_level)
            }
            _ => self.format_expr(expr),
        }
    }

    fn format_case_pretty(
        &self,
        operand: &Option<Box<Expr>>,
        whens: &[WhenClause],
        else_expr: &Option<Box<Expr>>,
        indent_level: usize,
    ) -> String {
        let pad = Self::pad(indent_level);
        let pad_inner = Self::pad(indent_level + 1);
        let mut lines = vec![self.kw("CASE").to_string()];
        if let Some(op) = operand {
            lines[0].push(' ');
            lines[0].push_str(&self.format_expr(op));
        }
        for w in whens {
            let mut when_line = format!("{}{} {}", pad_inner, self.kw("WHEN"), self.format_expr(&w.condition));
            let result_pretty = self.format_expr_pretty_in_select(&w.result, indent_level + 2);
            when_line.push_str(&format!(" {} {}", self.kw("THEN"), result_pretty));
            lines.push(when_line);
        }
        if let Some(e) = else_expr {
            lines.push(format!("{}{} {}", pad_inner, self.kw("ELSE"), self.format_expr(e)));
        }
        lines.push(format!("{}{}", pad, self.kw("END")));
        lines.join("\n")
    }

    fn format_expr_pretty(&self, expr: &Expr, indent_level: usize) -> String {
        match expr {
            Expr::BinaryOp { op, left, right } if matches!(op.to_uppercase().as_str(), "AND" | "OR") => {
                let cont_pad = Self::pad(indent_level);
                let left_str = self.format_expr_pretty(left, indent_level);
                let right_str = self.format_expr_pretty(right, indent_level);
                format!("{}\n{}{} {}", left_str, cont_pad, op.to_uppercase(), right_str)
            }
            Expr::Exists(subquery) => {
                let sub_pretty = self.format_select_pretty(subquery, indent_level + 1);
                let close_pad = Self::pad(indent_level - 1);
                format!("{} (\n{}\n{})", self.kw("EXISTS"), sub_pretty, close_pad)
            }
            Expr::Subquery(subquery) => {
                let sub_pretty = self.format_select_pretty(subquery, indent_level + 1);
                let close_pad = Self::pad(indent_level - 1);
                format!("(\n{}\n{})", sub_pretty, close_pad)
            }
            _ => self.format_expr(expr),
        }
    }

    fn format_select(&self, stmt: &SelectStatement) -> String {
        let mut parts = Vec::new();

        if let Some(with) = &stmt.with {
            parts.push(self.format_with(with));
        }

        let mut select_parts = Vec::new();
        select_parts.push(self.kw("SELECT"));
        if let Some(hints) = self.format_hints(&stmt.hints) {
            select_parts.push(hints);
        }
        if stmt.distinct {
            if stmt.distinct_on.is_empty() {
                select_parts.push(self.kw("DISTINCT"));
            } else {
                let cols: Vec<String> = stmt.distinct_on.iter().map(|e| self.format_expr(e)).collect();
                select_parts.push(format!("{} ON ({})", self.kw("DISTINCT"), cols.join(", ")));
            }
        }
        select_parts.push(self.format_select_targets(&stmt.targets));
        parts.push(select_parts.join(" "));

        if let Some(into_targets) = &stmt.into_targets {
            let prefix = if stmt.bulk_collect {
                format!("{} {} {}", self.kw("BULK"), self.kw("COLLECT"), self.kw("INTO"))
            } else {
                self.kw("INTO").to_string()
            };
            parts.push(format!("{} {}", prefix, self.format_select_targets(into_targets)));
        }

        if let Some(into_table) = &stmt.into_table {
            let mut into_parts = vec![self.kw("INTO")];
            if into_table.unlogged {
                into_parts.push(self.kw("UNLOGGED"));
            }
            into_parts.push(self.kw("TABLE"));
            into_parts.push(self.format_object_name(&into_table.table_name));
            parts.push(into_parts.join(" "));
        }

        if !stmt.from.is_empty() {
            parts.push(format!("{} {}", self.kw("FROM"), self.format_table_refs(&stmt.from)));
        }

        if let Some(where_clause) = &stmt.where_clause {
            parts.push(format!("{} {}", self.kw("WHERE"), self.format_expr(where_clause)));
        }

        if let Some(cb) = &stmt.connect_by {
            if let Some(sw) = &cb.start_with {
                parts.push(format!("{} {}", self.kw("START WITH"), self.format_expr(sw)));
            }
            let nocycle = if cb.nocycle { " NOCYCLE" } else { "" };
            parts.push(format!("{}{} {}", self.kw("CONNECT BY"), nocycle, self.format_expr(&cb.condition)));
        }

        if !stmt.group_by.is_empty() {
            parts.push(format!(
                "{} {}",
                self.kw("GROUP BY"),
                stmt.group_by.iter().map(|item| self.format_group_by_item(item)).collect::<Vec<_>>().join(", ")
            ));
        }

        if let Some(having) = &stmt.having {
            parts.push(format!("{} {}", self.kw("HAVING"), self.format_expr(having)));
        }

        let mut result = parts.join(" ");

        if let Some(set_op) = &stmt.set_operation {
            let (op_name, all, right) = match set_op {
                SetOperation::Union { all, right } => ("UNION", all, right),
                SetOperation::Intersect { all, right } => ("INTERSECT", all, right),
                SetOperation::Except { all, right } => ("EXCEPT", all, right),
            };
            let mut op_str = self.kw(op_name);
            if *all {
                op_str.push_str(" ALL");
            }
            result = format!("{} {} {}", result, op_str, self.format_select(right));
        }

        let mut final_parts = vec![result];

        if !stmt.order_by.is_empty() {
            final_parts.push(format!("{} {}", self.kw("ORDER BY"), self.format_order_by(&stmt.order_by)));
        }

        if !stmt.window_clause.is_empty() {
            let windows: Vec<String> = stmt
                .window_clause
                .iter()
                .map(|w| {
                    format!(
                        "{} {} ({})",
                        self.quote_identifier(&w.name),
                        self.kw("AS"),
                        self.format_window_spec(&w.spec)
                    )
                })
                .collect();
            final_parts.push(format!("{} {}", self.kw("WINDOW"), windows.join(", ")));
        }

        if let Some(limit) = &stmt.limit {
            final_parts.push(format!("{} {}", self.kw("LIMIT"), self.format_expr(limit)));
        }

        if let Some(offset) = &stmt.offset {
            final_parts.push(format!("{} {}", self.kw("OFFSET"), self.format_expr(offset)));
        }

        final_parts.join(" ")
    }

    fn format_with(&self, with: &WithClause) -> String {
        let mut parts = vec![self.kw("WITH")];
        if with.recursive {
            parts.push(self.kw("RECURSIVE"));
        }
        let ctes: Vec<String> = with.ctes.iter().map(|c| self.format_cte(c)).collect();
        parts.push(ctes.join(", "));
        parts.join(" ")
    }

    fn format_cte(&self, cte: &Cte) -> String {
        let mut parts = vec![self.quote_identifier(&cte.name)];
        if let Some(mat) = cte.materialized {
            if mat {
                parts.push(self.kw("MATERIALIZED"));
            } else {
                parts.push(self.kw("NOT MATERIALIZED"));
            }
        }
        if !cte.columns.is_empty() {
            let cols: Vec<String> = cte.columns.iter().map(|c| self.quote_identifier(c)).collect();
            parts.push(format!("({})", cols.join(", ")));
        }
        parts.push(self.kw("AS").to_string());
        parts.push(format!("({})", self.format_select(&cte.query)));
        parts.join(" ")
    }

    fn format_select_targets(&self, targets: &[SelectTarget]) -> String {
        let formatted: Vec<String> = targets.iter().map(|t| self.format_select_target(t)).collect();
        formatted.join(", ")
    }

    fn format_select_target(&self, target: &SelectTarget) -> String {
        match target {
            SelectTarget::Expr(expr, alias) => {
                let mut result = self.format_expr(expr);
                if let Some(a) = alias {
                    result = format!("{} AS {}", result, self.format_ident(a));
                }
                result
            }
            SelectTarget::Star(alias) => match alias {
                Some(a) => format!("{}.*", self.format_ident(a)),
                None => "*".to_string(),
            },
        }
    }

    fn format_table_refs(&self, refs: &[TableRef]) -> String {
        let formatted: Vec<String> = refs.iter().map(|r| self.format_table_ref(r)).collect();
        formatted.join(", ")
    }

    fn format_table_ref(&self, r: &TableRef) -> String {
        match r {
            TableRef::Table { name, alias, column_aliases, partition, timecapsule, tablesample } => {
                let mut result = self.format_object_name(name);
                if let Some(p) = partition {
                    let vals: Vec<String> = p.values.iter().map(|v| self.quote_identifier(v)).collect();
                    result = format!("{} PARTITION ({})", result, vals.join(", "));
                }
                if let Some(ts) = tablesample {
                    let args: Vec<String> = ts.arguments.iter().map(|a| self.format_expr(a)).collect();
                    result =
                        format!("{} TABLESAMPLE {} ({})", result, self.quote_identifier(&ts.method), args.join(", "));
                    if let Some(rep) = &ts.repeatable {
                        result = format!("{} REPEATABLE ({})", result, self.format_expr(rep));
                    }
                }
                if let Some(tc) = timecapsule {
                    result = format!("{} TIMECAPSULE {}", result, self.format_expr(tc));
                }
                if let Some(a) = alias {
                    result = format!("{} AS {}", result, self.format_ident(a));
                }
                if !column_aliases.is_empty() {
                    let cols: Vec<String> = column_aliases.iter().map(|c| self.quote_identifier(c)).collect();
                    result = format!("{}({})", result, cols.join(", "));
                }
                result
            }
            TableRef::FunctionCall { name, args, alias, column_defs, .. } => {
                let args_str: Vec<String> = args.iter().map(|a| self.format_expr(a)).collect();
                let mut result = format!("{}({})", self.format_object_name(name), args_str.join(", "));
                if let Some(a) = alias {
                    result = format!("{} AS {}", result, self.format_ident(a));
                }
                if !column_defs.is_empty() {
                    let defs: Vec<String> = column_defs.iter().map(|d| self.format_column_def(d)).collect();
                    result = format!("{}({})", result, defs.join(", "));
                }
                result
            }
            TableRef::Subquery { query, alias, lateral } => {
                let sub = format!("({})", self.format_select(query));
                let result = if *lateral { format!("{} {}", self.kw("LATERAL"), sub) } else { sub };
                match alias {
                    Some(a) => format!("{} AS {}", result, self.format_ident(a)),
                    None => result,
                }
            }
            TableRef::Values { values, alias, column_names, lateral } => {
                let vals = format!("({})", self.format_values(values));
                let result = if *lateral { format!("{} {}", self.kw("LATERAL"), vals) } else { vals };
                let alias_str = match alias {
                    Some(a) => {
                        let cols = if column_names.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "({})",
                                column_names.iter().map(|c| self.quote_identifier(c)).collect::<Vec<_>>().join(", ")
                            )
                        };
                        format!(" AS {}{}", self.format_ident(a), cols)
                    }
                    None => String::new(),
                };
                format!("{}{}", result, alias_str)
            }
            TableRef::Join { left, right, join_type, condition, natural, using_columns } => {
                let left_str = self.format_table_ref(left);
                let join_kw = match join_type {
                    JoinType::Inner => self.kw("INNER JOIN"),
                    JoinType::Left => self.kw("LEFT JOIN"),
                    JoinType::Right => self.kw("RIGHT JOIN"),
                    JoinType::Full => self.kw("FULL JOIN"),
                    JoinType::Cross => self.kw("CROSS JOIN"),
                };
                let right_str = self.format_table_ref(right);
                let natural_kw = if *natural { format!("{} ", self.kw("NATURAL")) } else { String::new() };
                let mut result = format!("{} {}{} {}", left_str, natural_kw, join_kw, right_str);
                if let Some(cond) = condition {
                    result = format!("{} {} {}", result, self.kw("ON"), self.format_expr(cond));
                } else if !using_columns.is_empty() {
                    result = format!("{} {} ({})", result, self.kw("USING"), using_columns.join(", "));
                }
                result
            }
            TableRef::Pivot { source, pivot } => {
                let source_str = self.format_table_ref(source);
                let values = pivot
                    .values
                    .iter()
                    .map(|v| {
                        let mut s = self.format_expr(&v.value);
                        if let Some(a) = &v.alias {
                            s = format!("{} {} {}", s, self.kw("AS"), a);
                        }
                        s
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let xml_str = if pivot.xml == Some(true) { format!(" {}", self.kw("XML")) } else { String::new() };
                format!(
                    "{} {}{} ({} {} {} {} ({}))",
                    source_str,
                    self.kw("PIVOT"),
                    xml_str,
                    self.format_expr(&pivot.aggregate),
                    self.kw("FOR"),
                    self.format_object_name(&pivot.for_column),
                    self.kw("IN"),
                    values
                )
            }
            TableRef::Unpivot { source, unpivot } => {
                let source_str = self.format_table_ref(source);
                let columns = unpivot
                    .columns
                    .iter()
                    .map(|v| {
                        let mut s = self.format_expr(&v.value);
                        if let Some(a) = &v.alias {
                            s = format!("{} {} {}", s, self.kw("AS"), a);
                        }
                        s
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let nulls_mod = match unpivot.include_nulls {
                    Some(true) => format!(" {} {}", self.kw("INCLUDE"), self.kw("NULLS")),
                    Some(false) => format!(" {} {}", self.kw("EXCLUDE"), self.kw("NULLS")),
                    None => String::new(),
                };
                format!(
                    "{} {}{} ({} {} {} ({}))",
                    source_str,
                    self.kw("UNPIVOT"),
                    nulls_mod,
                    self.format_object_name(&unpivot.value_column),
                    self.kw("FOR"),
                    self.format_object_name(&unpivot.for_column),
                    columns
                )
            }
        }
    }

    fn format_table_ref_with_partition(&self, table_ref: &TableRef, partition: Option<&DmlPartitionClause>) -> String {
        match table_ref {
            TableRef::Table { name, alias, column_aliases: _, partition: _, timecapsule: _, tablesample: _ } => {
                let mut parts = vec![self.format_object_name(name)];
                if let Some(p) = partition {
                    parts.push(self.format_dml_partition(p));
                }
                if let Some(a) = alias {
                    parts.push(format!("{} {}", self.kw("AS"), self.quote_identifier(a)));
                }
                parts.join(" ")
            }
            _ => {
                let mut result = self.format_table_ref(table_ref);
                if let Some(p) = partition {
                    result = format!("{} {}", result, self.format_dml_partition(p));
                }
                result
            }
        }
    }

    fn format_group_by_item(&self, item: &GroupByItem) -> String {
        match item {
            GroupByItem::Expr(e) => self.format_expr(e),
            GroupByItem::GroupingSets(sets) => {
                let inner: Vec<String> = sets
                    .iter()
                    .map(|s| {
                        if s.is_empty() {
                            "()".to_string()
                        } else {
                            format!("({})", s.iter().map(|e| self.format_expr(e)).collect::<Vec<_>>().join(", "))
                        }
                    })
                    .collect();
                format!("{} ({})", self.kw("GROUPING SETS"), inner.join(", "))
            }
            GroupByItem::Rollup(cols) => {
                format!(
                    "{} ({})",
                    self.kw("ROLLUP"),
                    cols.iter().map(|e| self.format_expr(e)).collect::<Vec<_>>().join(", ")
                )
            }
            GroupByItem::Cube(cols) => {
                format!(
                    "{} ({})",
                    self.kw("CUBE"),
                    cols.iter().map(|e| self.format_expr(e)).collect::<Vec<_>>().join(", ")
                )
            }
        }
    }

    fn format_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Literal(lit) => self.format_literal(lit),
            Expr::ColumnRef(name) => self.format_object_name(name),
            Expr::ColumnRefOuterJoin(name) => format!("{}(+)", self.format_object_name(name)),
            Expr::QualifiedStar(table) => format!("{}.*", self.quote_identifier_relaxed(table)),
            Expr::BinaryOp { left, op, right } => {
                format!("{} {} {}", self.format_expr(left), op.to_uppercase(), self.format_expr(right))
            }
            Expr::Like { expr, pattern, escape, negated, case_insensitive } => {
                let op = match (*negated, *case_insensitive) {
                    (false, false) => "LIKE",
                    (true, false) => "NOT LIKE",
                    (false, true) => "ILIKE",
                    (true, true) => "NOT ILIKE",
                };
                let mut result = format!("{} {} {}", self.format_expr(expr), self.kw(op), self.format_expr(pattern));
                if let Some(esc) = escape {
                    result = format!("{} {} {}", result, self.kw("ESCAPE"), self.format_expr(esc));
                }
                result
            }
            Expr::UnaryOp { op, expr } => {
                let op_upper = op.to_uppercase();
                if op == "-" || op == "@" {
                    format!("{}{}", op_upper, self.format_expr(expr))
                } else {
                    format!("{} {}", op_upper, self.format_expr(expr))
                }
            }
            Expr::AtTimeZone { expr, zone } => {
                format!("{} AT TIME ZONE {}", self.format_expr(expr), self.format_expr(zone))
            }
            Expr::FunctionCall {
                name,
                args,
                distinct,
                over,
                filter,
                within_group,
                separator,
                default,
                conversion_format,
                agg_from,
                ..
            } => {
                let parts: Vec<String> = name.iter().map(|i| self.format_ident(i)).collect();
                let mut result = format!("{}(", parts.join("."));
                if *distinct {
                    result.push_str(&format!("{} ", self.kw("DISTINCT")));
                }
                result.push_str(&self.format_exprs(args));
                if let Some(sep) = separator {
                    result.push_str(&format!(" {} {}", self.kw("SEPARATOR"), self.format_expr(sep)));
                }
                if let Some(def) = default {
                    result.push_str(&format!(
                        " {} {} {} {}",
                        self.kw("DEFAULT"),
                        self.format_expr(def),
                        self.kw("ON"),
                        self.kw("CONVERSION ERROR")
                    ));
                }
                if let Some(fmt) = conversion_format {
                    result.push_str(&format!(", {}", self.format_expr(fmt)));
                }
                if let Some(from_items) = agg_from {
                    result.push(' ');
                    result.push_str(&self.kw("FROM"));
                    result.push(' ');
                    result.push_str(&self.format_table_refs(from_items));
                }
                result.push(')');
                if let Some(f) = filter {
                    result = format!("{} {} ({})", result, self.kw("FILTER"), self.format_expr(f));
                }
                if !within_group.is_empty() {
                    let items: Vec<String> = within_group
                        .iter()
                        .map(|item| {
                            let mut s = self.format_expr(&item.expr);
                            if let Some(asc) = item.asc {
                                s = format!("{} {}", s, if asc { self.kw("ASC") } else { self.kw("DESC") });
                            }
                            s
                        })
                        .collect();
                    result = format!(
                        "{} {} ({} {})",
                        result,
                        self.kw("WITHIN GROUP"),
                        self.kw("ORDER BY"),
                        items.join(", ")
                    );
                }
                if let Some(ws) = over {
                    result = format!("{} {}", result, self.format_window_spec(ws));
                }
                result
            }
            Expr::SpecialFunction { name, args, .. } => {
                let lower = name.to_lowercase();
                match lower.as_str() {
                    "overlay" => {
                        let mut s = format!("{}(", self.kw("OVERLAY"));
                        if !args.is_empty() {
                            s.push_str(&self.format_expr(&args[0]));
                        }
                        if args.len() >= 2 {
                            s.push_str(&format!(" {} ", self.kw("PLACING")));
                            s.push_str(&self.format_expr(&args[1]));
                        }
                        if args.len() >= 3 {
                            s.push_str(&format!(" {} ", self.kw("FROM")));
                            s.push_str(&self.format_expr(&args[2]));
                        }
                        if args.len() >= 4 {
                            s.push_str(&format!(" {} ", self.kw("FOR")));
                            s.push_str(&self.format_expr(&args[3]));
                        }
                        s.push(')');
                        s
                    }
                    "position" => {
                        let mut s = format!("{}(", self.kw("POSITION"));
                        if !args.is_empty() {
                            s.push_str(&self.format_expr(&args[0]));
                        }
                        if args.len() >= 2 {
                            s.push_str(&format!(" {} ", self.kw("IN")));
                            s.push_str(&self.format_expr(&args[1]));
                        }
                        s.push(')');
                        s
                    }
                    "substring" | "substr" => {
                        let mut s = format!("{}(", self.kw("SUBSTRING"));
                        if !args.is_empty() {
                            s.push_str(&self.format_expr(&args[0]));
                        }
                        if args.len() >= 2 {
                            s.push_str(&format!(" {} ", self.kw("FROM")));
                            s.push_str(&self.format_expr(&args[1]));
                        }
                        if args.len() >= 3 {
                            s.push_str(&format!(" {} ", self.kw("FOR")));
                            s.push_str(&self.format_expr(&args[2]));
                        }
                        s.push(')');
                        s
                    }
                    "extract" => {
                        let mut s = format!("{}(", self.kw("EXTRACT"));
                        if !args.is_empty() {
                            s.push_str(&self.format_expr(&args[0]));
                        }
                        if args.len() >= 2 {
                            s.push_str(&format!(" {} ", self.kw("FROM")));
                            s.push_str(&self.format_expr(&args[1]));
                        }
                        s.push(')');
                        s
                    }
                    "trim" => {
                        let mut s = format!("{}(", self.kw("TRIM"));
                        if args.len() == 2 {
                            s.push_str(&self.format_expr(&args[0]));
                            s.push_str(&format!(" {} ", self.kw("FROM")));
                            s.push_str(&self.format_expr(&args[1]));
                        } else if args.len() >= 3 {
                            s.push_str(&self.format_expr(&args[0]));
                            s.push(' ');
                            s.push_str(&self.format_expr(&args[1]));
                            s.push_str(&format!(" {} ", self.kw("FROM")));
                            s.push_str(&self.format_expr(&args[2]));
                        } else if args.len() == 1 {
                            s.push_str(&self.format_expr(&args[0]));
                        }
                        s.push(')');
                        s
                    }
                    "interval" => {
                        let mut s = self.kw("INTERVAL");
                        if !args.is_empty() {
                            s.push(' ');
                            s.push_str(&self.format_expr(&args[0]));
                        }
                        if args.len() >= 2 {
                            s.push(' ');
                            s.push_str(&self.format_expr(&args[1]));
                        }
                        s
                    }
                    _ => format!("{}({})", name, self.format_exprs(args)),
                }
            }
            Expr::CurrentOf { cursor_name } => {
                format!("{} {} {}", self.kw("CURRENT"), self.kw("OF"), cursor_name)
            }
            Expr::Case { operand, whens, else_expr } => {
                let mut parts = vec![self.kw("CASE")];
                if let Some(op) = operand {
                    parts.push(self.format_expr(op));
                }
                for w in whens {
                    parts.push(format!("{} {} {}", self.kw("WHEN"), self.format_expr(&w.condition), self.kw("THEN")));
                    parts.push(self.format_expr(&w.result));
                }
                if let Some(e) = else_expr {
                    parts.push(self.kw("ELSE"));
                    parts.push(self.format_expr(e));
                }
                parts.push(self.kw("END"));
                parts.join(" ")
            }
            Expr::Between { expr, low, high, negated } => {
                let not_str = if *negated { format!("{} ", self.kw("NOT")) } else { String::new() };
                format!(
                    "{} {}{} {} {} {}",
                    self.format_expr(expr),
                    not_str,
                    self.kw("BETWEEN"),
                    self.format_expr(low),
                    self.kw("AND"),
                    self.format_expr(high)
                )
            }
            Expr::InList { expr, list, negated } => {
                let not_str = if *negated { format!("{} ", self.kw("NOT")) } else { String::new() };
                format!("{} {}{} ({})", self.format_expr(expr), not_str, self.kw("IN"), self.format_exprs(list))
            }
            Expr::InSubquery { expr, subquery, negated } => {
                let not_str = if *negated { format!("{} ", self.kw("NOT")) } else { String::new() };
                format!("{} {}{} ({})", self.format_expr(expr), not_str, self.kw("IN"), self.format_select(subquery))
            }
            Expr::Exists(subquery) => {
                format!("{} ({})", self.kw("EXISTS"), self.format_select(subquery))
            }
            Expr::Subquery(subquery) => {
                format!("({})", self.format_select(subquery))
            }
            Expr::ScalarSublink { expr, op, sublink_type, subquery } => {
                let type_str = match sublink_type {
                    crate::ast::ScalarSublinkType::Any => self.kw("ANY"),
                    crate::ast::ScalarSublinkType::Some => self.kw("SOME"),
                    crate::ast::ScalarSublinkType::All => self.kw("ALL"),
                };
                format!(
                    "{} {} {}({})",
                    self.format_expr(expr),
                    op.to_uppercase(),
                    type_str,
                    self.format_select(subquery)
                )
            }
            Expr::IsNull { expr, negated } => {
                let not_str = if *negated { format!(" {}", self.kw("NOT")) } else { String::new() };
                format!("{} IS{} {}", self.format_expr(expr), not_str, self.kw("NULL"))
            }
            Expr::IsBoolean { expr, value, negated } => {
                let not_str = if *negated { format!(" {}", self.kw("NOT")) } else { String::new() };
                let bool_str = if *value { self.kw("TRUE") } else { self.kw("FALSE") };
                format!("{} IS{} {}", self.format_expr(expr), not_str, bool_str)
            }
            Expr::TypeCast { expr, type_name, default, format: fmt_expr } => {
                let mut result = format!("{}::{}", self.format_expr(expr), self.format_data_type(type_name));
                if let Some(def) = default {
                    result = format!(
                        "{} {} {} {}",
                        result,
                        self.kw("DEFAULT"),
                        self.format_expr(def),
                        self.kw("ON CONVERSION ERROR")
                    );
                }
                if let Some(fmt) = fmt_expr {
                    result = format!("{}, {}", result, self.format_expr(fmt));
                }
                result
            }
            Expr::Treat { expr, type_name } => {
                format!(
                    "{}({} {} {})",
                    self.kw("TREAT"),
                    self.format_expr(expr),
                    self.kw("AS"),
                    self.format_data_type(type_name)
                )
            }
            Expr::CollationFor { expr } => {
                format!("{} {} ({})", self.kw("COLLATION"), self.kw("FOR"), self.format_expr(expr))
            }
            Expr::Parameter(n) => format!("${}", n),
            Expr::MyBatisParam(content) => format!("#{{{}}}", content),
            Expr::MyBatisRawExpr(content) => format!("${{{}}}", content),
            Expr::JdbcParam => "?".to_string(),
            Expr::Array(elements) => {
                format!("{}[{}]", self.kw("ARRAY"), self.format_exprs(elements))
            }
            Expr::Subscript { object, lower, upper, is_slice } => {
                if *is_slice {
                    let lower_str = match lower {
                        Some(l) => self.format_expr(l),
                        None => String::new(),
                    };
                    let upper_str = match upper {
                        Some(u) => self.format_expr(u),
                        None => String::new(),
                    };
                    format!("{}[{}:{}]", self.format_expr(object), lower_str, upper_str)
                } else {
                    let index = lower.as_ref().expect("non-slice subscript should have lower");
                    format!("{}[{}]", self.format_expr(object), self.format_expr(index))
                }
            }
            Expr::FieldAccess { object, field } => {
                format!("{}.{}", self.format_expr(object), self.quote_identifier(field))
            }
            Expr::Parenthesized(expr) => {
                format!("({})", self.format_expr(expr))
            }
            Expr::RowConstructor(elems) => {
                let formatted: Vec<String> = elems.iter().map(|e| self.format_expr(e)).collect();
                format!("({})", formatted.join(", "))
            }
            Expr::Prior(e) => format!("{} {}", self.kw("PRIOR"), self.format_expr(e)),
            Expr::Default => self.kw("DEFAULT").to_string(),
            Expr::XmlElement { entity_escaping, evalname, name, attributes, content } => {
                let mut result = self.kw("XMLELEMENT") + "(";
                if let Some(true) = entity_escaping {
                    result = result + &self.kw("ENTITYESCAPING") + " ";
                } else if let Some(false) = entity_escaping {
                    result = result + &self.kw("NOENTITYESCAPING") + " ";
                }
                if let Some(expr) = evalname {
                    result = result + &self.kw("EVALNAME") + " " + &self.format_expr(expr);
                } else if let Some(n) = name {
                    result = result + &self.quote_identifier_relaxed(n);
                }
                if let Some(attrs) = attributes {
                    result = result + ", " + &self.format_xml_attributes(attrs);
                }
                for item in content {
                    result = result + ", " + &self.format_expr(&item.expr);
                    if let Some(alias) = &item.alias {
                        result = result + " " + &self.kw("AS") + " " + &self.format_ident(alias);
                    }
                }
                result + ")"
            }
            Expr::XmlConcat(exprs) => {
                format!("{}({})", self.kw("XMLCONCAT"), self.format_exprs(exprs))
            }
            Expr::XmlForest(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| {
                        let mut s = self.format_expr(&item.expr);
                        if let Some(alias) = &item.alias {
                            s = s + " " + &self.kw("AS") + " " + &self.format_ident(alias);
                        }
                        s
                    })
                    .collect();
                format!("{}({})", self.kw("XMLFOREST"), parts.join(", "))
            }
            Expr::XmlParse { option, expr, wellformed } => {
                let opt_str = match option {
                    XmlOption::Document => self.kw("DOCUMENT"),
                    XmlOption::Content => self.kw("CONTENT"),
                };
                let mut result = format!("{}({} {}", self.kw("XMLPARSE"), opt_str, self.format_expr(expr));
                if *wellformed {
                    result = result + " " + &self.kw("WELLFORMED");
                }
                result + ")"
            }
            Expr::XmlPi { name, content } => {
                let mut result = self.kw("XMLPI") + "(" + &self.kw("NAME") + " ";
                if let Some(n) = name {
                    result = result + &self.quote_identifier_relaxed(n);
                }
                if let Some(c) = content {
                    result = result + ", " + &self.format_expr(c);
                }
                result + ")"
            }
            Expr::XmlRoot { expr, version, standalone } => {
                let mut result = format!("{}({}, {}", self.kw("XMLROOT"), self.format_expr(expr), self.kw("VERSION"));
                if let Some(v) = version {
                    result = result + " " + &self.format_expr(v);
                } else {
                    result = result + " " + &self.kw("NO") + " " + &self.kw("VALUE");
                }
                if let Some(s) = standalone {
                    result = result + ", " + &self.kw("STANDALONE") + " ";
                    match s {
                        Some(true) => result = result + &self.kw("YES"),
                        Some(false) => result = result + &self.kw("NO"),
                        None => result = result + &self.kw("NO") + " " + &self.kw("VALUE"),
                    }
                }
                result + ")"
            }
            Expr::XmlSerialize { option, expr, type_name } => {
                let opt_str = match option {
                    XmlOption::Document => self.kw("DOCUMENT"),
                    XmlOption::Content => self.kw("CONTENT"),
                };
                format!(
                    "{}({} {} {} {})",
                    self.kw("XMLSERIALIZE"),
                    opt_str,
                    self.format_expr(expr),
                    self.kw("AS"),
                    self.format_data_type(type_name)
                )
            }
            Expr::PredictBy { model_name, features } => {
                format!(
                    "{} {} {} ({} {})",
                    self.kw("PREDICT"),
                    self.kw("BY"),
                    model_name,
                    self.kw("FEATURES"),
                    features.iter().map(|f| self.format_expr(f)).collect::<Vec<_>>().join(", ")
                )
            }
            Expr::SysDate => self.kw("SYSDATE"),
            Expr::SequenceValue { sequence, function } => {
                let func_name = match function {
                    SequenceFunc::Nextval => "NEXTVAL",
                    SequenceFunc::Currval => "CURRVAL",
                };
                format!("{}.{}", self.format_object_name(sequence), func_name)
            }
            Expr::PlVariable(name) => self.format_object_name(name),
            Expr::CursorAttribute { cursor, attribute } => {
                let attr_str = match attribute {
                    crate::ast::CursorAttributeKind::NotFound => "%NOTFOUND",
                    crate::ast::CursorAttributeKind::Found => "%FOUND",
                    crate::ast::CursorAttributeKind::IsOpen => "%ISOPEN",
                    crate::ast::CursorAttributeKind::RowCount => "%ROWCOUNT",
                    crate::ast::CursorAttributeKind::BulkExceptions => "%BULK_EXCEPTIONS",
                };
                format!("{}{}", self.format_expr(cursor), attr_str)
            }
        }
    }

    fn format_xml_attributes(&self, attrs: &XmlAttributes) -> String {
        let mut result = self.kw("XMLATTRIBUTES") + "(";
        if let Some(true) = attrs.entity_escaping {
            result = result + &self.kw("ENTITYESCAPING") + " ";
        } else if let Some(false) = attrs.entity_escaping {
            result = result + &self.kw("NOENTITYESCAPING") + " ";
        }
        let parts: Vec<String> = attrs
            .items
            .iter()
            .map(|a| {
                let mut s = self.format_expr(&a.value);
                if let Some(name) = &a.name {
                    s = s + " " + &self.kw("AS") + " " + &self.format_ident(name);
                }
                s
            })
            .collect();
        result + &parts.join(", ") + ")"
    }

    fn format_exprs(&self, exprs: &[Expr]) -> String {
        let formatted: Vec<String> = exprs.iter().map(|e| self.format_expr(e)).collect();
        formatted.join(", ")
    }

    fn format_literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Integer(n) => n.to_string(),
            Literal::Float(s) => s.clone(),
            Literal::String(s) => self.quote_string(s),
            Literal::EscapeString(s) => {
                let escaped = s.replace('\\', "\\\\").replace("'", "''");
                format!("E'{}'", escaped)
            }
            Literal::BitString(s) => format!("B'{}'", s),
            Literal::HexString(s) => format!("X'{}'", s),
            Literal::NationalString(s) => format!("N'{}'", s),
            Literal::DollarString { tag, body } => match tag {
                Some(t) => format!("${t}${body}${t}$"),
                None => format!("$${body}$$"),
            },
            Literal::Boolean(b) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Literal::Null => "NULL".to_string(),
        }
    }

    fn quote_string(&self, s: &str) -> String {
        let escaped = s.replace("'", "''");
        format!("'{}'", escaped)
    }

    fn quote_identifier(&self, s: &str) -> String {
        if s == "*" {
            return s.to_string();
        }
        if s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !s.is_empty()
            && !s.chars().next().expect("s is non-empty after is_empty() check").is_ascii_digit()
        {
            s.to_string()
        } else {
            format!("\"{}\"", s.replace("\"", "\"\""))
        }
    }

    fn quote_identifier_relaxed(&self, s: &str) -> String {
        if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !s.is_empty()
            && !s.chars().next().expect("s is non-empty after is_empty() check").is_ascii_digit()
        {
            s.to_string()
        } else {
            format!("\"{}\"", s.replace("\"", "\"\""))
        }
    }

    fn format_ident(&self, ident: &Ident) -> String {
        match ident.quote_style {
            None => ident.value.clone(),
            Some(q) => {
                let escaped = ident.value.replace(q, &format!("{q}{q}"));
                format!("{q}{escaped}{q}")
            }
        }
    }

    fn format_object_name(&self, name: &ObjectName) -> String {
        let parts: Vec<String> = name.iter().map(|i| self.format_ident(i)).collect();
        parts.join(".")
    }

    fn format_window_spec(&self, ws: &WindowSpec) -> String {
        let mut parts = vec![self.kw("OVER")];
        let mut inner = Vec::new();

        if let Some(name) = &ws.window_name {
            inner.push(self.quote_identifier(name));
        }

        if !ws.partition_by.is_empty() {
            inner.push(format!("{} {}", self.kw("PARTITION BY"), self.format_exprs(&ws.partition_by)));
        }

        if !ws.order_by.is_empty() {
            inner.push(format!("{} {}", self.kw("ORDER BY"), self.format_order_by(&ws.order_by)));
        }

        if let Some(frame) = &ws.frame {
            inner.push(self.format_window_frame(frame));
        }

        if inner.len() == 1 && ws.window_name.is_some() {
            parts.push(inner.pop().expect("inner has exactly 1 element when window_name is Some"));
        } else {
            parts.push(format!("({})", inner.join(" ")));
        }

        parts.join(" ")
    }

    fn format_window_frame(&self, frame: &WindowFrame) -> String {
        let mode_str = match frame.mode {
            WindowFrameMode::Rows => self.kw("ROWS"),
            WindowFrameMode::Range => self.kw("RANGE"),
            WindowFrameMode::Groups => self.kw("GROUPS"),
        };
        let mut parts = vec![mode_str];

        if let Some(start) = &frame.start {
            if let Some(end) = &frame.end {
                parts.push(self.kw("BETWEEN"));
                parts.push(self.format_frame_bound(start));
                parts.push(self.kw("AND"));
                parts.push(self.format_frame_bound(end));
            } else {
                parts.push(self.format_frame_bound(start));
            }
        }

        parts.join(" ")
    }

    fn format_frame_bound(&self, bound: &WindowFrameBound) -> String {
        match &bound.direction {
            WindowFrameDirection::UnboundedPreceding => self.kw("UNBOUNDED PRECEDING"),
            WindowFrameDirection::UnboundedFollowing => self.kw("UNBOUNDED FOLLOWING"),
            WindowFrameDirection::CurrentRow => self.kw("CURRENT ROW"),
            WindowFrameDirection::Preceding(offset) => {
                format!("{} {}", offset, self.kw("PRECEDING"))
            }
            WindowFrameDirection::Following(offset) => {
                format!("{} {}", offset, self.kw("FOLLOWING"))
            }
        }
    }

    fn format_order_by(&self, items: &[OrderByItem]) -> String {
        let formatted: Vec<String> = items.iter().map(|i| self.format_order_by_item(i)).collect();
        formatted.join(", ")
    }

    fn format_order_by_item(&self, item: &OrderByItem) -> String {
        let mut result = self.format_expr(&item.expr);
        if let Some(asc) = item.asc {
            result = format!("{} {}", result, if asc { self.kw("ASC") } else { self.kw("DESC") });
        }
        if let Some(nf) = item.nulls_first {
            result = format!("{} {} {}", result, self.kw("NULLS"), if nf { self.kw("FIRST") } else { self.kw("LAST") });
        }
        if let Some(using) = &item.using {
            result = format!("{} {} {}", result, self.kw("USING"), self.format_expr(using));
        }
        result
    }

    fn format_hints(&self, hints: &[HintInfo]) -> Option<String> {
        if hints.is_empty() {
            return None;
        }
        let formatted: Vec<String> = hints
            .iter()
            .map(|h| {
                let mut s = if h.negated { format!("no {}", h.name) } else { h.name.clone() };
                if let Some(ref qb) = h.queryblock {
                    s.push(' ');
                    s.push_str(qb);
                }
                if let Some(ref args) = h.args {
                    s.push('(');
                    s.push_str(args);
                    s.push(')');
                }
                s
            })
            .collect();
        Some(format!("/*+ {} */", formatted.join(" ")))
    }

    fn format_insert(&self, stmt: &InsertStatement) -> String {
        let mut parts = Vec::new();
        if let Some(ref w) = stmt.with {
            parts.push(self.format_with(w));
        }
        parts.push(self.kw("INSERT"));
        if let Some(hints) = self.format_hints(&stmt.hints) {
            parts.push(hints);
        }
        parts.push(self.kw("INTO"));
        parts.push(self.format_object_name(&stmt.table));

        if let Some(ref alias) = stmt.alias {
            parts.push(self.format_ident(alias));
        }

        if let Some(ref p) = stmt.partition {
            parts.push(self.format_dml_partition(p));
        }

        if !stmt.columns.is_empty() {
            parts.push(format!("({})", stmt.columns.join(", ")));
        }

        match &stmt.source {
            InsertSource::Values(rows) => {
                let rows_formatted: Vec<String> =
                    rows.iter().map(|row| format!("({})", self.format_exprs(row))).collect();
                parts.push(format!("{} {}", self.kw("VALUES"), rows_formatted.join(", ")));
            }
            InsertSource::Select(sel) => {
                parts.push(self.format_select(sel));
            }
            InsertSource::DefaultValues => {
                parts.push(self.kw("DEFAULT VALUES"));
            }
            InsertSource::RecordVariable(expr) => {
                parts.push(format!("{} {}", self.kw("VALUES"), self.format_expr(expr)));
            }
            InsertSource::Set(assignments) => {
                parts.push(self.kw("SET"));
                let assign_strs: Vec<String> = assignments
                    .iter()
                    .map(|a| {
                        let col = self.format_object_name(&a.columns[0]);
                        format!("{} = {}", col, self.format_expr(&a.value))
                    })
                    .collect();
                parts.push(assign_strs.join(", "));
            }
        }

        if let Some(ref dup_key) = stmt.on_duplicate_key {
            match dup_key {
                OnDuplicateKeyUpdate::Nothing => {
                    parts.push(
                        [self.kw("ON"), self.kw("DUPLICATE"), self.kw("KEY"), self.kw("UPDATE"), self.kw("NOTHING")]
                            .join(" "),
                    );
                }
                OnDuplicateKeyUpdate::Update { assignments, where_clause } => {
                    let mut dup_parts = vec![self.kw("ON"), self.kw("DUPLICATE"), self.kw("KEY"), self.kw("UPDATE")];
                    let assign_strs: Vec<String> = assignments
                        .iter()
                        .map(|a| format!("{} = {}", self.format_object_name(&a.columns[0]), self.format_expr(&a.value)))
                        .collect();
                    dup_parts.push(assign_strs.join(", "));
                    if let Some(w) = where_clause {
                        dup_parts.push(self.kw("WHERE"));
                        dup_parts.push(self.format_expr(w));
                    }
                    parts.push(dup_parts.join(" "));
                }
            }
        }

        if let Some(ref on_conflict) = stmt.on_conflict {
            let mut conflict_parts = vec![self.kw("ON"), self.kw("CONFLICT")];
            match &on_conflict.target {
                Some(ConflictTarget::Columns(cols)) => {
                    let col_strs: Vec<String> = cols.iter().map(|c| self.quote_identifier(c)).collect();
                    conflict_parts.push(format!("({})", col_strs.join(", ")));
                }
                Some(ConflictTarget::OnConstraint(name)) => {
                    conflict_parts.push(format!("{} {}", self.kw("ON CONSTRAINT"), self.quote_identifier(name)));
                }
                None => {}
            }
            conflict_parts.push(self.kw("DO"));
            match &on_conflict.action {
                ConflictAction::DoNothing => {
                    conflict_parts.push(self.kw("NOTHING"));
                }
                ConflictAction::DoUpdate { assignments, where_clause } => {
                    conflict_parts.push(self.kw("UPDATE"));
                    conflict_parts.push(self.kw("SET"));
                    let assigns: Vec<String> = assignments
                        .iter()
                        .map(|a| format!("{} = {}", self.format_object_name(&a.columns[0]), self.format_expr(&a.value)))
                        .collect();
                    conflict_parts.push(assigns.join(", "));
                    if let Some(ref wc) = where_clause {
                        conflict_parts.push(self.kw("WHERE"));
                        conflict_parts.push(self.format_expr(wc));
                    }
                }
            }
            parts.push(conflict_parts.join(" "));
        }

        if !stmt.returning.is_empty() {
            parts.push(format!("{} {}", self.kw("RETURNING"), self.format_select_targets(&stmt.returning)));
        }

        if let Some(into_targets) = &stmt.into_targets {
            let prefix = if stmt.bulk_collect {
                format!("{} {} {}", self.kw("BULK"), self.kw("COLLECT"), self.kw("INTO"))
            } else {
                self.kw("INTO").to_string()
            };
            parts.push(format!("{} {}", prefix, self.format_select_targets(into_targets)));
        }

        parts.join(" ")
    }

    fn format_insert_all_target(&self, target: &InsertAllTarget) -> String {
        let mut parts = vec![self.kw("INTO"), self.format_object_name(&target.table)];
        if !target.columns.is_empty() {
            parts.push(format!("({})", target.columns.join(", ")));
        }
        let rows: Vec<String> = target.values.iter().map(|row| format!("({})", self.format_exprs(row))).collect();
        parts.push(format!("{} {}", self.kw("VALUES"), rows.join(", ")));
        parts.join(" ")
    }

    fn format_insert_all(&self, stmt: &InsertAllStatement) -> String {
        let mut parts = vec![self.kw("INSERT ALL")];
        for target in &stmt.targets {
            parts.push(self.format_insert_all_target(target));
        }
        for cond in &stmt.conditions {
            parts.push(self.kw("WHEN"));
            parts.push(self.format_expr(&cond.condition));
            parts.push(self.kw("THEN"));
            for target in &cond.targets {
                parts.push(self.format_insert_all_target(target));
            }
        }
        if !stmt.else_targets.is_empty() {
            parts.push(self.kw("ELSE"));
            for target in &stmt.else_targets {
                parts.push(self.format_insert_all_target(target));
            }
        }
        parts.push(self.format_select(&stmt.source));
        parts.join(" ")
    }

    fn format_insert_first(&self, stmt: &InsertFirstStatement) -> String {
        let mut parts = vec![self.kw("INSERT FIRST")];
        for cond in &stmt.when_clauses {
            parts.push(self.kw("WHEN"));
            parts.push(self.format_expr(&cond.condition));
            parts.push(self.kw("THEN"));
            for target in &cond.targets {
                parts.push(self.format_insert_all_target(target));
            }
        }
        if !stmt.else_targets.is_empty() {
            parts.push(self.kw("ELSE"));
            for target in &stmt.else_targets {
                parts.push(self.format_insert_all_target(target));
            }
        }
        parts.push(self.format_select(&stmt.source));
        parts.join(" ")
    }

    fn format_update(&self, stmt: &UpdateStatement) -> String {
        let mut parts = Vec::new();
        if let Some(ref w) = stmt.with {
            parts.push(self.format_with(w));
        }
        parts.push(self.kw("UPDATE"));
        if let Some(hints) = self.format_hints(&stmt.hints) {
            parts.push(hints);
        }
        parts.push(self.format_table_refs(&stmt.tables));

        if let Some(ref p) = stmt.partition {
            parts.push(self.format_dml_partition(p));
        }

        let assignments: Vec<String> = stmt
            .assignments
            .iter()
            .map(|a| {
                if a.columns.len() == 1 {
                    format!("{} = {}", self.format_object_name(&a.columns[0]), self.format_expr(&a.value))
                } else {
                    let cols: Vec<String> = a.columns.iter().map(|c| self.format_object_name(c)).collect();
                    format!("({}) = {}", cols.join(", "), self.format_expr(&a.value))
                }
            })
            .collect();
        parts.push(format!("{} {}", self.kw("SET"), assignments.join(", ")));

        if !stmt.from.is_empty() {
            parts.push(format!("{} {}", self.kw("FROM"), self.format_table_refs(&stmt.from)));
        }

        if let Some(where_clause) = &stmt.where_clause {
            parts.push(format!("{} {}", self.kw("WHERE"), self.format_expr(where_clause)));
        }

        if let Some(order_by) = &stmt.order_by {
            let items: Vec<String> = order_by
                .iter()
                .map(|item| {
                    let mut s = self.format_expr(&item.expr);
                    match item.asc {
                        Some(true) => {
                            s.push(' ');
                            s.push_str(&self.kw("ASC"));
                        }
                        Some(false) => {
                            s.push(' ');
                            s.push_str(&self.kw("DESC"));
                        }
                        None => {}
                    }
                    if let Some(nf) = item.nulls_first {
                        s.push(' ');
                        if nf {
                            s.push_str(&self.kw("NULLS FIRST"));
                        } else {
                            s.push_str(&self.kw("NULLS LAST"));
                        }
                    }
                    if let Some(ref using) = item.using {
                        s.push(' ');
                        s.push_str(&self.kw("USING"));
                        s.push(' ');
                        s.push_str(&self.format_expr(using));
                    }
                    s
                })
                .collect();
            parts.push(format!("{} {}", self.kw("ORDER BY"), items.join(", ")));
        }

        if let Some(limit) = &stmt.limit {
            parts.push(format!("{} {}", self.kw("LIMIT"), self.format_expr(limit)));
        }

        if !stmt.returning.is_empty() {
            parts.push(format!("{} {}", self.kw("RETURNING"), self.format_select_targets(&stmt.returning)));
        }

        if let Some(into_targets) = &stmt.into_targets {
            let prefix = if stmt.bulk_collect {
                format!("{} {} {}", self.kw("BULK"), self.kw("COLLECT"), self.kw("INTO"))
            } else {
                self.kw("INTO").to_string()
            };
            parts.push(format!("{} {}", prefix, self.format_select_targets(into_targets)));
        }

        parts.join(" ")
    }

    fn format_delete(&self, stmt: &DeleteStatement) -> String {
        let mut parts = Vec::new();
        if let Some(ref w) = stmt.with {
            parts.push(self.format_with(w));
        }
        parts.push(self.kw("DELETE"));
        if let Some(hints) = self.format_hints(&stmt.hints) {
            parts.push(hints);
        }
        parts.push(self.kw("FROM"));
        parts.push(self.format_table_refs(&stmt.tables));

        if !stmt.using.is_empty() {
            parts.push(format!("{} {}", self.kw("USING"), self.format_table_refs(&stmt.using)));
        }

        if let Some(where_clause) = &stmt.where_clause {
            parts.push(format!("{} {}", self.kw("WHERE"), self.format_expr(where_clause)));
        }

        if let Some(order_by) = &stmt.order_by {
            let items: Vec<String> = order_by
                .iter()
                .map(|item| {
                    let mut s = self.format_expr(&item.expr);
                    match item.asc {
                        Some(true) => {
                            s.push(' ');
                            s.push_str(&self.kw("ASC"));
                        }
                        Some(false) => {
                            s.push(' ');
                            s.push_str(&self.kw("DESC"));
                        }
                        None => {}
                    }
                    if let Some(nf) = item.nulls_first {
                        s.push(' ');
                        if nf {
                            s.push_str(&self.kw("NULLS FIRST"));
                        } else {
                            s.push_str(&self.kw("NULLS LAST"));
                        }
                    }
                    if let Some(ref using) = item.using {
                        s.push(' ');
                        s.push_str(&self.kw("USING"));
                        s.push(' ');
                        s.push_str(&self.format_expr(using));
                    }
                    s
                })
                .collect();
            parts.push(format!("{} {}", self.kw("ORDER BY"), items.join(", ")));
        }

        if let Some(limit) = &stmt.limit {
            parts.push(format!("{} {}", self.kw("LIMIT"), self.format_expr(limit)));
        }

        if !stmt.returning.is_empty() {
            parts.push(format!("{} {}", self.kw("RETURNING"), self.format_select_targets(&stmt.returning)));
        }

        if let Some(into_targets) = &stmt.into_targets {
            let prefix = if stmt.bulk_collect {
                format!("{} {} {}", self.kw("BULK"), self.kw("COLLECT"), self.kw("INTO"))
            } else {
                self.kw("INTO").to_string()
            };
            parts.push(format!("{} {}", prefix, self.format_select_targets(into_targets)));
        }

        parts.join(" ")
    }

    fn format_merge(&self, stmt: &MergeStatement) -> String {
        let mut parts = Vec::new();
        parts.push(self.kw("MERGE"));
        if let Some(hints) = self.format_hints(&stmt.hints) {
            parts.push(hints);
        }
        parts.push(self.kw("INTO"));
        parts.push(self.format_table_ref_with_partition(&stmt.target, stmt.partition.as_ref()));

        parts.push(self.kw("USING"));
        parts.push(self.format_table_ref_with_partition(&stmt.source, stmt.source_partition.as_ref()));
        parts.push(self.kw("ON"));
        parts.push(self.format_expr(&stmt.on_condition));

        for wc in &stmt.when_clauses {
            let when_str = if wc.matched {
                format!("{} {}", self.kw("WHEN MATCHED THEN"), self.format_merge_action(&wc.action))
            } else {
                format!("{} {}", self.kw("WHEN NOT MATCHED THEN"), self.format_merge_action(&wc.action))
            };
            parts.push(when_str);
        }

        parts.join(" ")
    }

    fn format_merge_action(&self, action: &MergeAction) -> String {
        match action {
            MergeAction::Update(assignments) => {
                let assigns: Vec<String> = assignments
                    .iter()
                    .map(|a| format!("{} = {}", self.format_object_name(&a.columns[0]), self.format_expr(&a.value)))
                    .collect();
                format!("{} {}", self.kw("UPDATE SET"), assigns.join(", "))
            }
            MergeAction::Delete => self.kw("DELETE").to_string(),
            MergeAction::Insert { columns, values } => {
                let mut parts = vec![self.kw("INSERT")];
                if !columns.is_empty() {
                    let col_strs: Vec<String> = columns.iter().map(|c| self.format_object_name(c)).collect();
                    parts.push(format!("({})", col_strs.join(", ")));
                }
                parts.push(format!("{} ({})", self.kw("VALUES"), self.format_exprs(values)));
                parts.join(" ")
            }
        }
    }

    fn format_create_table(&self, stmt: &CreateTableStatement) -> String {
        let mut parts = vec![self.kw("CREATE TABLE")];

        if stmt.if_not_exists {
            parts.push(self.kw("IF NOT EXISTS"));
        }

        parts.push(self.format_object_name(&stmt.name));

        let mut inner = Vec::new();
        for col in &stmt.columns {
            inner.push(self.format_column_def(col));
        }
        for constraint in &stmt.constraints {
            inner.push(self.format_table_constraint(constraint));
        }
        for like in &stmt.like_clauses {
            inner.push(self.format_like_clause(like));
        }

        parts.push(format!("({})", inner.join(", ")));

        if let Some(pb) = &stmt.partition_by {
            parts.push(self.format_partition_clause(pb));
        }

        if let Some(spb) = &stmt.subpartition_by {
            parts.push(self.format_subpartition_clause(spb));
        }

        if let Some(n) = stmt.subpartitions_count {
            parts.push(format!("{} {}", self.kw("SUBPARTITIONS"), n));
        }

        if let Some(ts) = &stmt.tablespace {
            parts.push(format!("{} {}", self.kw("TABLESPACE"), ts));
        }

        if let Some(true) = stmt.compress {
            parts.push(self.kw("COMPRESS"));
        } else if let Some(false) = stmt.compress {
            parts.push(self.kw("NOCOMPRESS"));
        }

        if let Some(oc) = &stmt.on_commit {
            parts.push(format!(
                "{} {}",
                self.kw("ON COMMIT"),
                match oc {
                    OnCommitAction::PreserveRows => self.kw("PRESERVE ROWS"),
                    OnCommitAction::DeleteRows => self.kw("DELETE ROWS"),
                    OnCommitAction::Drop => self.kw("DROP"),
                }
            ));
        }

        if let Some(d) = &stmt.distribute_by {
            parts.push(self.format_distribute_clause(d));
        }

        if let Some(g) = &stmt.to_group {
            parts.push(format!("{} {} {}", self.kw("TO"), self.kw("GROUP"), g));
        }

        if !stmt.options.is_empty() {
            let opts: Vec<String> = stmt.options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
            parts.push(format!("{} ({})", self.kw("WITH"), opts.join(", ")));
        }

        if !stmt.table_options.is_empty() {
            let opts: Vec<String> = stmt.table_options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
            parts.push(format!("{} ({})", self.kw("TABLE OPTION"), opts.join(", ")));
        }

        if let Some(ilm) = &stmt.ilm {
            let mut ilm_parts = vec![self.kw("ILM ADD POLICY ROW STORE COMPRESS ADVANCED ROW")];
            ilm_parts.push(self.kw("AFTER"));
            ilm_parts.push(format!("{} {} {}", ilm.after_n, ilm.unit, self.kw("OF NO MODIFICATION")));
            if let Some(cond) = &ilm.condition {
                ilm_parts.push(format!("{} ({})", self.kw("ON"), self.format_expr(cond)));
            }
            parts.push(ilm_parts.join(" "));
        }

        if let Some(rm) = stmt.row_movement {
            if rm {
                parts.push(self.kw("ENABLE ROW MOVEMENT"));
            } else {
                parts.push(self.kw("DISABLE ROW MOVEMENT"));
            }
        }

        parts.join(" ")
    }

    fn format_distribute_clause(&self, clause: &DistributeClause) -> String {
        match clause {
            DistributeClause::Hash { columns } => {
                format!("{} {} ({})", self.kw("DISTRIBUTE BY HASH"), "", columns.join(", "))
            }
            DistributeClause::Replication => self.kw("DISTRIBUTE BY REPLICATION"),
            DistributeClause::RoundRobin { columns } => {
                format!("{} {} ({})", self.kw("DISTRIBUTE BY ROUNDROBIN"), "", columns.join(", "))
            }
            DistributeClause::Modulo { columns } => {
                format!("{} {} ({})", self.kw("DISTRIBUTE BY MODULO"), "", columns.join(", "))
            }
        }
    }

    fn format_dml_partition(&self, clause: &DmlPartitionClause) -> String {
        match clause {
            DmlPartitionClause::Partition(names) => {
                format!("{} ({})", self.kw("PARTITION"), names.join(", "))
            }
            DmlPartitionClause::Subpartition(names) => {
                format!("{} ({})", self.kw("SUBPARTITION"), names.join(", "))
            }
            DmlPartitionClause::PartitionFor(exprs) => {
                let formatted: Vec<String> = exprs.iter().map(|e| self.format_expr(e)).collect();
                format!("{} {} ({})", self.kw("PARTITION"), self.kw("FOR"), formatted.join(", "))
            }
            DmlPartitionClause::SubpartitionFor(exprs) => {
                let formatted: Vec<String> = exprs.iter().map(|e| self.format_expr(e)).collect();
                format!("{} {} ({})", self.kw("SUBPARTITION"), self.kw("FOR"), formatted.join(", "))
            }
        }
    }

    fn format_partition_clause(&self, clause: &PartitionClause) -> String {
        match clause {
            PartitionClause::Range { columns, interval, is_columns, partitions_count, partitions } => {
                let mut parts = vec![
                    if *is_columns { self.kw("PARTITION BY RANGE COLUMNS") } else { self.kw("PARTITION BY RANGE") },
                    format!("({})", columns.iter().map(|c| self.format_object_name(c)).collect::<Vec<_>>().join(", ")),
                ];
                if let Some(iv) = interval {
                    parts.push(format!("{} ({})", self.kw("INTERVAL"), self.format_expr(iv)));
                }
                if let Some(n) = partitions_count {
                    parts.push(format!("{} {}", self.kw("PARTITIONS"), n));
                }
                if !partitions.is_empty() {
                    parts.push(self.format_partition_defs(partitions));
                }
                parts.join(" ")
            }
            PartitionClause::List { columns, is_columns, partitions } => {
                let mut parts = vec![
                    if *is_columns { self.kw("PARTITION BY LIST COLUMNS") } else { self.kw("PARTITION BY LIST") },
                    format!("({})", columns.iter().map(|c| self.format_object_name(c)).collect::<Vec<_>>().join(", ")),
                ];
                if !partitions.is_empty() {
                    parts.push(self.format_partition_defs(partitions));
                }
                parts.join(" ")
            }
            PartitionClause::Hash { columns, partitions_count, partitions } => {
                let mut parts = vec![
                    self.kw("PARTITION BY HASH"),
                    format!("({})", columns.iter().map(|c| self.format_object_name(c)).collect::<Vec<_>>().join(", ")),
                ];
                if let Some(n) = partitions_count {
                    parts.push(format!("{} {}", self.kw("PARTITIONS"), n));
                }
                if !partitions.is_empty() {
                    parts.push(self.format_partition_defs(partitions));
                }
                parts.join(" ")
            }
        }
    }

    fn format_subpartition_clause(&self, clause: &PartitionClause) -> String {
        match clause {
            PartitionClause::Range { columns, is_columns, partitions, .. } => {
                let mut parts = vec![
                    if *is_columns {
                        self.kw("SUBPARTITION BY RANGE COLUMNS")
                    } else {
                        self.kw("SUBPARTITION BY RANGE")
                    },
                    format!("({})", columns.iter().map(|c| self.format_object_name(c)).collect::<Vec<_>>().join(", ")),
                ];
                if !partitions.is_empty() {
                    parts.push(self.format_partition_defs(partitions));
                }
                parts.join(" ")
            }
            PartitionClause::List { columns, is_columns, partitions } => {
                let mut parts = vec![
                    if *is_columns { self.kw("SUBPARTITION BY LIST COLUMNS") } else { self.kw("SUBPARTITION BY LIST") },
                    format!("({})", columns.iter().map(|c| self.format_object_name(c)).collect::<Vec<_>>().join(", ")),
                ];
                if !partitions.is_empty() {
                    parts.push(self.format_partition_defs(partitions));
                }
                parts.join(" ")
            }
            PartitionClause::Hash { columns, partitions_count, partitions } => {
                let mut parts = vec![
                    self.kw("SUBPARTITION BY HASH"),
                    format!("({})", columns.iter().map(|c| self.format_object_name(c)).collect::<Vec<_>>().join(", ")),
                ];
                if let Some(n) = partitions_count {
                    parts.push(format!("{} {}", self.kw("SUBPARTITIONS"), n));
                }
                if !partitions.is_empty() {
                    parts.push(self.format_partition_defs(partitions));
                }
                parts.join(" ")
            }
        }
    }

    fn format_partition_defs(&self, defs: &[PartitionDef]) -> String {
        let parts: Vec<String> = defs
            .iter()
            .map(|d| {
                let mut s = format!("{} {}", self.kw("PARTITION"), self.quote_identifier(&d.name));
                if let Some(v) = &d.values {
                    s = format!("{} {}", s, self.format_partition_values(v));
                }
                if let Some(ts) = &d.tablespace {
                    s = format!("{} {} {}", s, self.kw("TABLESPACE"), self.quote_identifier(ts));
                }
                if !d.subpartitions.is_empty() {
                    let subs: Vec<String> = d
                        .subpartitions
                        .iter()
                        .map(|sp| {
                            let mut ss = format!("{} {}", self.kw("SUBPARTITION"), self.quote_identifier(&sp.name));
                            if let Some(v) = &sp.values {
                                ss = format!("{} {}", ss, self.format_partition_values(v));
                            }
                            if let Some(ts) = &sp.tablespace {
                                ss = format!("{} {} {}", ss, self.kw("TABLESPACE"), self.quote_identifier(ts));
                            }
                            ss
                        })
                        .collect();
                    s = format!("{} ({})", s, subs.join(", "));
                }
                s
            })
            .collect();
        format!("({})", parts.join(", "))
    }

    fn format_column_def(&self, col: &ColumnDef) -> String {
        let mut parts = vec![self.quote_identifier(&col.name)];
        parts.push(self.format_data_type(&col.data_type));

        if let Some(cm) = &col.compress_mode {
            parts.push(cm.to_uppercase());
        }

        if let Some(cs) = &col.charset {
            parts.push(format!("{} {}", self.kw("CHARSET"), cs));
        }

        if let Some(co) = &col.collate {
            parts.push(format!("{} {}", self.kw("COLLATE"), co));
        }

        for constraint in &col.constraints {
            parts.push(self.format_column_constraint(constraint));
        }

        if let Some(ou) = &col.on_update {
            parts.push(format!("{} {} {}", self.kw("ON"), self.kw("UPDATE"), ou));
        }

        if let Some(c) = &col.comment {
            parts.push(format!("{} {}", self.kw("COMMENT"), c));
        }

        if let Some(g) = &col.generated {
            let stored_str = if g.stored { format!(" {}", self.kw("STORED")) } else { String::new() };
            parts.push(format!(
                "{} {} {} ({}){}",
                self.kw("GENERATED"),
                self.kw("ALWAYS"),
                self.kw("AS"),
                self.format_expr(&g.expr),
                stored_str
            ));
        }

        if let Some(ew) = &col.encrypted_with {
            parts.push(format!(
                "{} {} ({} = {}, {} = {})",
                self.kw("ENCRYPTED"),
                self.kw("WITH"),
                "COLUMN_ENCRYPTION_KEY",
                ew.column_encryption_key,
                "ENCRYPTION_TYPE",
                ew.encryption_type
            ));
        }

        parts.join(" ")
    }

    fn format_data_type(&self, dt: &DataType) -> String {
        match dt {
            DataType::Boolean => "BOOLEAN".to_string(),
            DataType::SmallInt(p) => self.format_int_type("SMALLINT", p),
            DataType::Integer(p) => self.format_int_type("INTEGER", p),
            DataType::BigInt(p) => self.format_int_type("BIGINT", p),
            DataType::Real => "REAL".to_string(),
            DataType::Double => "DOUBLE PRECISION".to_string(),
            DataType::Numeric(p, s) => match (p, s) {
                (None, None) => "NUMERIC".to_string(),
                (Some(p), None) => format!("NUMERIC({})", p),
                (Some(p), Some(s)) => format!("NUMERIC({}, {})", p, s),
                (None, Some(s)) => format!("NUMERIC({}, {})", 0, s),
            },
            DataType::Char(n) => match n {
                Some(len) => format!("CHAR({})", len),
                None => "CHAR".to_string(),
            },
            DataType::Varchar(n) => match n {
                Some(len) => format!("VARCHAR({})", len),
                None => "VARCHAR".to_string(),
            },
            DataType::Text => "TEXT".to_string(),
            DataType::Bytea => "BYTEA".to_string(),
            DataType::Timestamp(p, tz) => {
                let precision = match p {
                    Some(pr) => format!("({})", pr),
                    None => String::new(),
                };
                let tz_str = match tz {
                    Some(TimeZoneInfo::WithTimeZone) => " WITH TIME ZONE",
                    Some(TimeZoneInfo::WithoutTimeZone) => " WITHOUT TIME ZONE",
                    None => "",
                };
                format!("TIMESTAMP{}{}", precision, tz_str)
            }
            DataType::Timestamptz(p) => match p {
                Some(pr) => format!("TIMESTAMPTZ({})", pr),
                None => "TIMESTAMPTZ".to_string(),
            },
            DataType::Date => "DATE".to_string(),
            DataType::Time(p, tz) => {
                let precision = match p {
                    Some(pr) => format!("({})", pr),
                    None => String::new(),
                };
                let tz_str = match tz {
                    Some(TimeZoneInfo::WithTimeZone) => " WITH TIME ZONE",
                    Some(TimeZoneInfo::WithoutTimeZone) => " WITHOUT TIME ZONE",
                    None => "",
                };
                format!("TIME{}{}", precision, tz_str)
            }
            DataType::Interval(it) => match it {
                Some(i) => {
                    let mut s = format!("INTERVAL {}", i.from);
                    if let Some(p) = i.from_precision {
                        s = format!("{}({})", s, p);
                    }
                    if let Some(to) = &i.to {
                        s = format!("{} TO {}", s, to);
                        if let Some(p) = i.to_precision {
                            s = format!("{}({})", s, p);
                        }
                    }
                    s
                }
                None => "INTERVAL".to_string(),
            },
            DataType::Json => "JSON".to_string(),
            DataType::Jsonb => "JSONB".to_string(),
            DataType::Uuid => "UUID".to_string(),
            DataType::Bit(n) => match n {
                Some(len) => format!("BIT({})", len),
                None => "BIT".to_string(),
            },
            DataType::Varbit(n) => match n {
                Some(len) => format!("BIT VARYING({})", len),
                None => "BIT VARYING".to_string(),
            },
            DataType::TinyInt(p) => self.format_int_type("TINYINT", p),
            DataType::Float(n) => match n {
                Some(len) => format!("FLOAT({})", len),
                None => "FLOAT".to_string(),
            },
            DataType::Serial => "SERIAL".to_string(),
            DataType::SmallSerial => "SMALLSERIAL".to_string(),
            DataType::BigSerial => "BIGSERIAL".to_string(),
            DataType::BinaryFloat => "BINARY_FLOAT".to_string(),
            DataType::BinaryDouble => "BINARY_DOUBLE".to_string(),
            DataType::Array(inner) => format!("{}[]", self.format_data_type(inner)),
            DataType::Custom(name, args) => {
                let base = self.format_object_name(name);
                if args.is_empty() {
                    base
                } else {
                    let args_str = args.iter().map(|a| self.format_expr(a)).collect::<Vec<_>>().join(", ");
                    format!("{}({})", base, args_str)
                }
            }
        }
    }

    fn format_int_type(&self, name: &str, precision: &Option<u32>) -> String {
        match precision {
            Some(n) => format!("{}({})", name, n),
            None => name.to_string(),
        }
    }

    fn format_column_constraint(&self, c: &ColumnConstraint) -> String {
        match c {
            ColumnConstraint::NotNull => self.kw("NOT NULL"),
            ColumnConstraint::Null => self.kw("NULL"),
            ColumnConstraint::Default(expr) => {
                format!("{} {}", self.kw("DEFAULT"), self.format_expr(expr))
            }
            ColumnConstraint::Unique => self.kw("UNIQUE"),
            ColumnConstraint::PrimaryKey => self.kw("PRIMARY KEY"),
            ColumnConstraint::Check(expr) => {
                format!("{} ({})", self.kw("CHECK"), self.format_expr(expr))
            }
            ColumnConstraint::References { ref_table, ref_columns, on_delete, on_update } => {
                let mut result = format!("{} {}", self.kw("REFERENCES"), self.format_object_name(ref_table));
                if !ref_columns.is_empty() {
                    result = format!("{} ({})", result, ref_columns.join(", "));
                }
                if let Some(action) = on_delete {
                    result = format!("{} {} {}", result, self.kw("ON"), self.kw("DELETE"));
                    result = format!("{} {}", result, self.format_referential_action(action));
                }
                if let Some(action) = on_update {
                    result = format!("{} {} {}", result, self.kw("ON"), self.kw("UPDATE"));
                    result = format!("{} {}", result, self.format_referential_action(action));
                }
                result
            }
        }
    }

    fn format_like_clause(&self, like: &LikeClause) -> String {
        let mut parts = vec![self.kw("LIKE"), self.format_object_name(&like.source_table)];
        for (is_including, opt) in &like.options {
            if *is_including {
                parts.push(format!("{} {}", self.kw("INCLUDING"), opt));
            } else {
                parts.push(format!("{} {}", self.kw("EXCLUDING"), opt));
            }
        }
        parts.join(" ")
    }

    fn format_table_constraint(&self, c: &TableConstraint) -> String {
        match c {
            TableConstraint::PrimaryKey { columns, using_index } => {
                let mut s = format!("{} ({})", self.kw("PRIMARY KEY"), columns.join(", "));
                if let Some(ui) = using_index {
                    if ui.is_empty() {
                        s = format!("{} {}", s, self.kw("USING INDEX"));
                    } else {
                        s = format!("{} {} {}", s, self.kw("USING INDEX"), ui);
                    }
                }
                s
            }
            TableConstraint::Unique { columns, deferrable, with_options, using_index } => {
                let mut s = format!("{} ({})", self.kw("UNIQUE"), columns.join(", "));
                if *deferrable {
                    s = format!("{} {}", s, self.kw("DEFERRABLE"));
                }
                if !with_options.is_empty() {
                    s = format!("{} {}", s, self.format_options(with_options));
                }
                if let Some(ui) = using_index {
                    if ui.is_empty() {
                        s = format!("{} {}", s, self.kw("USING INDEX"));
                    } else {
                        s = format!("{} {} {}", s, self.kw("USING INDEX"), ui);
                    }
                }
                s
            }
            TableConstraint::Check(expr) => {
                format!("{} ({})", self.kw("CHECK"), self.format_expr(expr))
            }
            TableConstraint::ForeignKey { columns, ref_table, ref_columns, on_delete, on_update } => {
                let mut result = format!(
                    "{} ({}) {} {}",
                    self.kw("FOREIGN KEY"),
                    columns.join(", "),
                    self.kw("REFERENCES"),
                    self.format_object_name(ref_table)
                );
                if !ref_columns.is_empty() {
                    result = format!("{} ({})", result, ref_columns.join(", "));
                }
                if let Some(action) = on_delete {
                    result = format!("{} {} {}", result, self.kw("ON DELETE"), self.format_referential_action(action));
                }
                if let Some(action) = on_update {
                    result = format!("{} {} {}", result, self.kw("ON UPDATE"), self.format_referential_action(action));
                }
                result
            }
        }
    }

    fn format_referential_action(&self, action: &ReferentialAction) -> &'static str {
        match action {
            ReferentialAction::Cascade => "CASCADE",
            ReferentialAction::Restrict => "RESTRICT",
            ReferentialAction::SetNull => "SET NULL",
            ReferentialAction::SetDefault => "SET DEFAULT",
            ReferentialAction::NoAction => "NO ACTION",
        }
    }

    fn format_alter_table(&self, stmt: &AlterTableStatement) -> String {
        let mut parts = vec![self.kw("ALTER TABLE")];

        if stmt.if_exists {
            parts.push(self.kw("IF EXISTS"));
        }

        parts.push(self.format_object_name(&stmt.name));

        let actions: Vec<String> = stmt.actions.iter().map(|a| self.format_alter_table_action(a)).collect();
        parts.push(actions.join(", "));

        parts.join(" ")
    }

    fn append_update_index_clauses(
        &self,
        parts: &mut Vec<String>,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    ) {
        if update_global_index {
            parts.push(self.kw("UPDATE GLOBAL INDEX"));
        }
        if let Some(val) = update_distributed_global_index {
            if val {
                parts.push(self.kw("UPDATE DISTRIBUTED GLOBAL INDEX"));
            } else {
                parts.push(self.kw("NO UPDATE DISTRIBUTED GLOBAL INDEX"));
            }
        }
    }

    fn format_alter_table_action(&self, action: &AlterTableAction) -> String {
        match action {
            AlterTableAction::AddColumn(col) => {
                format!("{} {}", self.kw("ADD COLUMN"), self.format_column_def(col))
            }
            AlterTableAction::DropColumn { name, if_exists, cascade } => {
                let mut parts = vec![self.kw("DROP COLUMN")];
                if *if_exists {
                    parts.push(self.kw("IF EXISTS"));
                }
                parts.push(self.quote_identifier(name));
                if *cascade {
                    parts.push(self.kw("CASCADE"));
                }
                parts.join(" ")
            }
            AlterTableAction::AlterColumn { name, action } => {
                format!(
                    "{} {} {}",
                    self.kw("ALTER COLUMN"),
                    self.quote_identifier(name),
                    self.format_alter_column_action(action)
                )
            }
            AlterTableAction::AddConstraint { constraint, .. } => {
                format!("{} {}", self.kw("ADD"), self.format_table_constraint(constraint))
            }
            AlterTableAction::AddConstraintIfExists { name } => {
                format!(
                    "{} {} {} {}",
                    self.kw("ADD CONSTRAINT"),
                    self.quote_identifier(name),
                    self.kw("IF"),
                    self.kw("EXISTS")
                )
            }
            AlterTableAction::DropConstraint { name, if_exists, cascade } => {
                let mut parts = vec![self.kw("DROP CONSTRAINT")];
                if *if_exists {
                    parts.push(self.kw("IF EXISTS"));
                }
                parts.push(self.quote_identifier(name));
                if *cascade {
                    parts.push(self.kw("CASCADE"));
                }
                parts.join(" ")
            }
            AlterTableAction::RenameColumn { old, new } => {
                format!(
                    "{} {} {} {}",
                    self.kw("RENAME COLUMN"),
                    self.quote_identifier(old),
                    self.kw("TO"),
                    self.quote_identifier(new)
                )
            }
            AlterTableAction::RenameTo { new_name } => {
                format!("{} {}", self.kw("RENAME TO"), self.quote_identifier(new_name))
            }
            AlterTableAction::OwnerTo { owner } => {
                format!("{} {}", self.kw("OWNER TO"), self.quote_identifier(owner))
            }
            AlterTableAction::SetSchema { schema } => {
                format!("{} {}", self.kw("SET SCHEMA"), self.quote_identifier(schema))
            }
            AlterTableAction::SetOptions { options } => {
                let pairs: Vec<String> = options
                    .iter()
                    .map(|(k, v)| format!("{} = {}", self.quote_identifier(k), self.quote_identifier(v)))
                    .collect();
                format!("{} ({})", self.kw("SET"), pairs.join(", "))
            }
            AlterTableAction::SetTablespace { tablespace } => {
                format!("{} {}", self.kw("SET TABLESPACE"), self.quote_identifier(tablespace))
            }
            AlterTableAction::SetWithoutOids => self.kw("SET WITHOUT OIDS").to_string(),
            AlterTableAction::ResetOptions { options } => {
                let names: Vec<String> = options.iter().map(|o| self.quote_identifier(o)).collect();
                format!("{} ({})", self.kw("RESET"), names.join(", "))
            }
            AlterTableAction::AddPartition { name, values, tablespace } => {
                let mut parts =
                    vec![self.kw("ADD PARTITION"), self.quote_identifier(name), self.format_partition_values(values)];
                if let Some(ts) = tablespace {
                    parts.push(format!("{} {}", self.kw("TABLESPACE"), ts));
                }
                parts.join(" ")
            }
            AlterTableAction::DropPartition {
                name,
                if_exists,
                update_global_index,
                update_distributed_global_index,
            } => {
                let mut parts = vec![self.kw("DROP PARTITION")];
                if *if_exists {
                    parts.push(self.kw("IF EXISTS"));
                }
                parts.push(self.quote_identifier(name));
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::TruncatePartition {
                name,
                for_values,
                cascade,
                update_global_index,
                update_distributed_global_index,
            } => {
                let partition_ref = if let Some(vals) = for_values {
                    let val_strs: Vec<String> = vals.iter().map(|v| self.format_expr(v)).collect();
                    format!("{} FOR ({})", self.kw("TRUNCATE PARTITION"), val_strs.join(", "))
                } else {
                    format!("{} {}", self.kw("TRUNCATE PARTITION"), self.quote_identifier(name))
                };
                let mut parts = vec![partition_ref];
                if *cascade {
                    parts.push(self.kw("CASCADE"));
                }
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::MergePartitions {
                names,
                into_name,
                update_global_index,
                update_distributed_global_index,
            } => {
                let name_list: Vec<String> = names.iter().map(|n| self.quote_identifier(n)).collect();
                let mut parts = vec![
                    format!("{} {}", self.kw("MERGE PARTITIONS"), name_list.join(", ")),
                    format!("{} {}", self.kw("INTO PARTITION"), self.quote_identifier(into_name)),
                ];
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::SplitPartition {
                name,
                at_value,
                into,
                update_global_index,
                update_distributed_global_index,
            } => {
                let mut parts = vec![self.kw("SPLIT PARTITION"), self.quote_identifier(name)];
                if let Some(at) = at_value {
                    parts.push(format!("{} {}", self.kw("AT"), self.format_expr(at)));
                }
                let partitions: Vec<String> = into
                    .iter()
                    .map(|p| {
                        let mut s = format!("{} {}", self.kw("PARTITION"), self.quote_identifier(&p.name));
                        if let Some(v) = &p.values {
                            s = format!("{} {}", s, self.format_partition_values(v));
                        }
                        s
                    })
                    .collect();
                parts.push(format!("{} ({})", self.kw("INTO"), partitions.join(", ")));
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::ExchangePartition {
                name,
                table,
                update_global_index,
                update_distributed_global_index,
                with_validation,
                verbose,
            } => {
                let mut parts = vec![
                    self.kw("EXCHANGE PARTITION"),
                    self.quote_identifier(name),
                    self.kw("WITH TABLE"),
                    self.format_object_name(table),
                ];
                if let Some(wv) = with_validation {
                    if *wv {
                        parts.push(self.kw("WITH VALIDATION"));
                    } else {
                        parts.push(self.kw("WITHOUT VALIDATION"));
                    }
                }
                if *verbose {
                    parts.push(self.kw("VERBOSE"));
                }
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::RenamePartition { old_name, new_name } => {
                format!(
                    "{} {} {} {}",
                    self.kw("RENAME PARTITION"),
                    self.quote_identifier(old_name),
                    self.kw("TO"),
                    self.quote_identifier(new_name)
                )
            }
            AlterTableAction::AddSubPartition { partition_name: _, name, values } => {
                let mut parts = vec![self.kw("ADD SUBPARTITION"), self.quote_identifier(name)];
                if let Some(v) = values {
                    parts.push(self.format_partition_values(v));
                }
                parts.join(" ")
            }
            AlterTableAction::DropSubPartition { name, if_exists } => {
                let mut parts = vec![self.kw("DROP SUBPARTITION")];
                if *if_exists {
                    parts.push(self.kw("IF EXISTS"));
                }
                parts.push(self.quote_identifier(name));
                parts.join(" ")
            }
            AlterTableAction::TruncateSubPartition { name, cascade } => {
                let mut parts = vec![self.kw("TRUNCATE SUBPARTITION"), self.quote_identifier(name)];
                if *cascade {
                    parts.push(self.kw("CASCADE"));
                }
                parts.join(" ")
            }
            AlterTableAction::MergeSubPartitions { names, into_name } => {
                let name_list: Vec<String> = names.iter().map(|n| self.quote_identifier(n)).collect();
                format!(
                    "{} {} {} {}",
                    self.kw("MERGE SUBPARTITIONS"),
                    name_list.join(", "),
                    self.kw("INTO SUBPARTITION"),
                    self.quote_identifier(into_name)
                )
            }
            AlterTableAction::SplitSubPartition { name, at_value, into } => {
                let mut parts = vec![self.kw("SPLIT SUBPARTITION"), self.quote_identifier(name)];
                if let Some(at) = at_value {
                    parts.push(format!("{} {}", self.kw("AT"), self.format_expr(at)));
                }
                let subs: Vec<String> = into
                    .iter()
                    .map(|p| {
                        let mut s = format!("{} {}", self.kw("SUBPARTITION"), self.quote_identifier(&p.name));
                        if let Some(v) = &p.values {
                            s = format!("{} {}", s, self.format_partition_values(v));
                        }
                        if let Some(ts) = &p.tablespace {
                            s = format!("{} {} {}", s, self.kw("TABLESPACE"), self.quote_identifier(ts));
                        }
                        s
                    })
                    .collect();
                parts.push(format!("{} ({})", self.kw("INTO"), subs.join(", ")));
                parts.join(" ")
            }
            AlterTableAction::ExchangeSubPartition { name, table } => {
                format!(
                    "{} {} {} {}",
                    self.kw("EXCHANGE SUBPARTITION"),
                    self.quote_identifier(name),
                    self.kw("WITH TABLE"),
                    self.format_object_name(table)
                )
            }
            AlterTableAction::RenameSubPartition { old_name, new_name } => {
                format!(
                    "{} {} {} {}",
                    self.kw("RENAME SUBPARTITION"),
                    self.quote_identifier(old_name),
                    self.kw("TO"),
                    self.quote_identifier(new_name)
                )
            }
            AlterTableAction::MoveSubPartition { name, tablespace } => {
                format!(
                    "{} {} {} {}",
                    self.kw("MOVE SUBPARTITION"),
                    self.quote_identifier(name),
                    self.kw("TABLESPACE"),
                    self.quote_identifier(tablespace)
                )
            }
            AlterTableAction::MovePartition { name, tablespace } => {
                format!(
                    "{} {} {} {}",
                    self.kw("MOVE PARTITION"),
                    self.quote_identifier(name),
                    self.kw("TABLESPACE"),
                    self.quote_identifier(tablespace)
                )
            }
            AlterTableAction::MovePartitionFor { expr, tablespace } => {
                format!(
                    "{} {} ({}) {} {}",
                    self.kw("MOVE PARTITION"),
                    self.kw("FOR"),
                    self.format_expr(expr),
                    self.kw("TABLESPACE"),
                    self.quote_identifier(tablespace)
                )
            }
            AlterTableAction::SplitPartitionFor {
                expr,
                at_value,
                into,
                update_global_index,
                update_distributed_global_index,
            } => {
                let mut parts =
                    vec![self.kw("SPLIT PARTITION"), format!("{} ({})", self.kw("FOR"), self.format_expr(expr))];
                if let Some(at) = at_value {
                    parts.push(format!("{} {}", self.kw("AT"), self.format_expr(at)));
                }
                let partitions: Vec<String> = into
                    .iter()
                    .map(|p| {
                        let mut s = format!("{} {}", self.kw("PARTITION"), self.quote_identifier(&p.name));
                        if let Some(v) = &p.values {
                            s = format!("{} {}", s, self.format_partition_values(v));
                        }
                        s
                    })
                    .collect();
                parts.push(format!("{} ({})", self.kw("INTO"), partitions.join(", ")));
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::DropPartitionFor {
                expr,
                if_exists,
                update_global_index,
                update_distributed_global_index,
            } => {
                let mut parts = vec![self.kw("DROP PARTITION")];
                if *if_exists {
                    parts.push(self.kw("IF EXISTS"));
                }
                parts.push(format!("{} ({})", self.kw("FOR"), self.format_expr(expr)));
                self.append_update_index_clauses(&mut parts, *update_global_index, *update_distributed_global_index);
                parts.join(" ")
            }
            AlterTableAction::RenamePartitionFor { expr, new_name } => {
                format!(
                    "{} {} ({}) {} {}",
                    self.kw("RENAME PARTITION"),
                    self.kw("FOR"),
                    self.format_expr(expr),
                    self.kw("TO"),
                    self.quote_identifier(new_name)
                )
            }
            AlterTableAction::EnableRowLevelSecurity => self.kw("ENABLE ROW LEVEL SECURITY").to_string(),
            AlterTableAction::DisableRowLevelSecurity => self.kw("DISABLE ROW LEVEL SECURITY").to_string(),
            AlterTableAction::EnableRowMovement => self.kw("ENABLE ROW MOVEMENT").to_string(),
            AlterTableAction::DisableRowMovement => self.kw("DISABLE ROW MOVEMENT").to_string(),
            AlterTableAction::SetCharset { charset, collation } => {
                if let Some(col) = collation {
                    format!(
                        "{} {} {} {}",
                        self.kw("CHARSET"),
                        self.quote_identifier(charset),
                        self.kw("COLLATE"),
                        self.quote_identifier(col)
                    )
                } else {
                    format!("{} {}", self.kw("CHARSET"), self.quote_identifier(charset))
                }
            }
            AlterTableAction::EnableTrigger { name } => {
                format!(
                    "{} {}",
                    self.kw("ENABLE TRIGGER"),
                    name.as_deref().map(|n| self.quote_identifier(n)).unwrap_or_else(|| self.kw("ALL").to_string())
                )
            }
            AlterTableAction::DisableTrigger { name } => {
                format!(
                    "{} {}",
                    self.kw("DISABLE TRIGGER"),
                    name.as_deref().map(|n| self.quote_identifier(n)).unwrap_or_else(|| self.kw("ALL").to_string())
                )
            }
            AlterTableAction::ValidateConstraint { name } => {
                format!("{} {}", self.kw("VALIDATE CONSTRAINT"), self.quote_identifier(name))
            }
            AlterTableAction::AddConstraintUsingIndex { name, index_name } => {
                format!(
                    "{} {} {} {}",
                    self.kw("ADD CONSTRAINT"),
                    self.quote_identifier(name),
                    self.kw("UNIQUE USING INDEX"),
                    self.quote_identifier(index_name)
                )
            }
            AlterTableAction::Inherit { parent } => {
                format!("{} {}", self.kw("INHERIT"), self.format_object_name(parent))
            }
            AlterTableAction::NoInherit { parent } => {
                format!("{} {}", self.kw("NO INHERIT"), self.format_object_name(parent))
            }
            AlterTableAction::ClusterOn { index_name } => {
                format!("{} {}", self.kw("CLUSTER ON"), self.quote_identifier(index_name))
            }
            AlterTableAction::SetWithoutCluster => self.kw("SET WITHOUT CLUSTER").to_string(),
            AlterTableAction::ReplicaIdentity(identity) => match identity {
                ReplicaIdentity::Default => {
                    format!("{} {}", self.kw("REPLICA IDENTITY"), self.kw("DEFAULT"))
                }
                ReplicaIdentity::Nothing => {
                    format!("{} {}", self.kw("REPLICA IDENTITY"), self.kw("NOTHING"))
                }
                ReplicaIdentity::Full => {
                    format!("{} {}", self.kw("REPLICA IDENTITY"), self.kw("FULL"))
                }
                ReplicaIdentity::Index { name } => {
                    format!(
                        "{} {} {}",
                        self.kw("REPLICA IDENTITY"),
                        self.kw("USING INDEX"),
                        self.quote_identifier(name)
                    )
                }
            },
            AlterTableAction::SetCompress => self.kw("SET COMPRESS").to_string(),
            AlterTableAction::SetNoCompress => self.kw("NOCOMPRESS").to_string(),
            AlterTableAction::ForceRowLevelSecurity => self.kw("FORCE ROW LEVEL SECURITY").to_string(),
            AlterTableAction::NoForceRowLevelSecurity => self.kw("NO FORCE ROW LEVEL SECURITY").to_string(),
            AlterTableAction::OfType { type_name } => {
                format!("{} {}", self.kw("OF"), self.format_object_name(type_name))
            }
            AlterTableAction::NotOfType { type_name } => {
                format!("{} {}", self.kw("NOT OF"), self.format_object_name(type_name))
            }
            AlterTableAction::AddNode { node_name } => {
                format!("{} {}", self.kw("ADD NODE"), self.quote_identifier(node_name))
            }
            AlterTableAction::DeleteNode { node_name } => {
                format!("{} {}", self.kw("DELETE NODE"), self.quote_identifier(node_name))
            }
            AlterTableAction::SetComment { comment } => {
                format!("{} = {}", self.kw("COMMENT"), self.quote_string(comment))
            }
            AlterTableAction::IlmAddPolicy(policy) => {
                let mut ilm_parts = vec![self.kw("ILM ADD POLICY ROW STORE COMPRESS ADVANCED ROW")];
                ilm_parts.push(self.kw("AFTER"));
                ilm_parts.push(format!("{} {} {}", policy.after_n, policy.unit, self.kw("OF NO MODIFICATION")));
                if let Some(cond) = &policy.condition {
                    ilm_parts.push(format!("{} ({})", self.kw("ON"), self.format_expr(cond)));
                }
                ilm_parts.join(" ")
            }
            AlterTableAction::IlmEnablePolicy => self.kw("ILM ENABLE POLICY").to_string(),
            AlterTableAction::IlmEnableAllPolicies => self.kw("ILM ENABLE_ALL").to_string(),
            AlterTableAction::IlmDisablePolicy => self.kw("ILM DISABLE POLICY").to_string(),
            AlterTableAction::IlmDisableAllPolicies => self.kw("ILM DISABLE_ALL").to_string(),
            AlterTableAction::IlmDeletePolicy => self.kw("ILM DELETE POLICY").to_string(),
            AlterTableAction::IlmDeleteAllPolicies => self.kw("ILM DELETE_ALL").to_string(),
            AlterTableAction::ModifyColumns(cols) => {
                let defs: Vec<String> = cols
                    .iter()
                    .map(|c| {
                        let mut s =
                            format!("{} {}", self.quote_identifier(&c.name), self.format_data_type(&c.data_type));
                        match c.nullability {
                            Some(false) => s = format!("{} {}", s, self.kw("NOT NULL")),
                            Some(true) => s = format!("{} {}", s, self.kw("NULL")),
                            None => {}
                        }
                        s
                    })
                    .collect();
                format!("{} ({})", self.kw("MODIFY"), defs.join(", "))
            }
            AlterTableAction::ModifyPartition { name, action } => {
                format!(
                    "{} {} {} {}",
                    self.kw("MODIFY"),
                    self.kw("PARTITION"),
                    self.quote_identifier(name),
                    self.format_alter_table_action(action)
                )
            }
            AlterTableAction::ModifySubPartition { name, action } => {
                format!(
                    "{} {} {} {}",
                    self.kw("MODIFY"),
                    self.kw("SUBPARTITION"),
                    self.quote_identifier(name),
                    self.format_alter_table_action(action)
                )
            }
            AlterTableAction::AddColumns(cols) => {
                let defs: Vec<String> = cols.iter().map(|c| self.format_column_def(c)).collect();
                format!("{} ({})", self.kw("ADD"), defs.join(", "))
            }
            AlterTableAction::StatisticsOp { op, columns } => {
                let op_str = match op {
                    StatisticsOpKind::Add => self.kw("ADD"),
                    StatisticsOpKind::Delete => self.kw("DELETE"),
                    StatisticsOpKind::Enable => self.kw("ENABLE"),
                    StatisticsOpKind::Disable => self.kw("DISABLE"),
                };
                let cols: Vec<String> = columns.iter().map(|c| self.quote_identifier(c)).collect();
                format!("{} {} (({}))", op_str, self.kw("STATISTICS"), cols.join(", "))
            }
            AlterTableAction::AlterColumnStatistics { column, percent, value } => {
                if *percent {
                    format!(
                        "{} {} {} {}",
                        self.kw("ALTER COLUMN"),
                        self.quote_identifier(column),
                        self.kw("SET STATISTICS PERCENT"),
                        value
                    )
                } else {
                    format!(
                        "{} {} {} {}",
                        self.kw("ALTER COLUMN"),
                        self.quote_identifier(column),
                        self.kw("SET STATISTICS"),
                        value
                    )
                }
            }
            AlterTableAction::AlterColumnStorage { column, storage } => {
                format!(
                    "{} {} {} {}",
                    self.kw("ALTER COLUMN"),
                    self.quote_identifier(column),
                    self.kw("SET STORAGE"),
                    storage.to_uppercase()
                )
            }
            AlterTableAction::GsiWaitAll => self.kw("GSIWAITALL").to_string(),
            AlterTableAction::EncryptionKeyRotation => self.kw("ENCRYPTION KEY ROTATION").to_string(),
            AlterTableAction::SetRule { enable, mode, name } => {
                let mut parts = vec![];
                if *enable {
                    parts.push(self.kw("ENABLE").to_string());
                } else {
                    parts.push(self.kw("DISABLE").to_string());
                }
                if let Some(m) = mode {
                    parts.push(m.to_uppercase());
                }
                parts.push(self.kw("RULE").to_string());
                parts.push(self.quote_identifier(name));
                parts.join(" ")
            }
            _ => "...".to_string(),
        }
    }

    fn format_alter_tablespace(&self, stmt: &AlterTablespaceStatement) -> String {
        let mut parts = vec![self.kw("ALTER TABLESPACE"), self.quote_identifier(&stmt.name)];
        match &stmt.action {
            AlterTablespaceAction::RenameTo { new_name } => {
                parts.push(self.kw("RENAME TO"));
                parts.push(self.quote_identifier(new_name));
            }
            AlterTablespaceAction::OwnerTo { new_owner } => {
                parts.push(self.kw("OWNER TO"));
                parts.push(self.quote_identifier(new_owner));
            }
            AlterTablespaceAction::SetOptions { options } => {
                let pairs: Vec<String> = options
                    .iter()
                    .map(|(k, v)| format!("{} = {}", self.quote_identifier(k), self.quote_identifier(v)))
                    .collect();
                parts.push(format!("{} ({})", self.kw("SET"), pairs.join(", ")));
            }
            AlterTablespaceAction::ResetOptions { options } => {
                let names: Vec<String> = options.iter().map(|o| self.quote_identifier(o)).collect();
                parts.push(format!("{} ({})", self.kw("RESET"), names.join(", ")));
            }
        }
        parts.join(" ")
    }

    fn format_partition_values(&self, values: &PartitionValues) -> String {
        match values {
            PartitionValues::LessThan(exprs) => {
                let formatted: Vec<String> = exprs.iter().map(|e| self.format_expr(e)).collect();
                format!("{} ({})", self.kw("VALUES LESS THAN"), formatted.join(", "))
            }
            PartitionValues::InValues(exprs) => {
                let formatted: Vec<String> = exprs.iter().map(|e| self.format_expr(e)).collect();
                format!("{} ({})", self.kw("VALUES IN"), formatted.join(", "))
            }
            PartitionValues::StartEnd { start, end, every } => {
                let mut s = format!(
                    "{} ({}) {} ({})",
                    self.kw("START"),
                    self.format_expr(start),
                    self.kw("END"),
                    self.format_expr(end)
                );
                if let Some(e) = every {
                    s = format!("{} {} ({})", s, self.kw("EVERY"), self.format_expr(e));
                }
                s
            }
            PartitionValues::StartOnly { start } => {
                format!("{} ({})", self.kw("START"), self.format_expr(start))
            }
            PartitionValues::EndOnly { end, every } => {
                let mut s = format!("{} ({})", self.kw("END"), self.format_expr(end));
                if let Some(e) = every {
                    s = format!("{} {} ({})", s, self.kw("EVERY"), self.format_expr(e));
                }
                s
            }
        }
    }

    fn format_alter_column_action(&self, action: &AlterColumnAction) -> String {
        match action {
            AlterColumnAction::SetDataType(dt) => {
                format!("{} {}", self.kw("TYPE"), self.format_data_type(dt))
            }
            AlterColumnAction::SetDefault(expr) => {
                format!("{} {}", self.kw("SET DEFAULT"), self.format_expr(expr))
            }
            AlterColumnAction::DropDefault => self.kw("DROP DEFAULT"),
            AlterColumnAction::SetNotNull => self.kw("SET NOT NULL"),
            AlterColumnAction::DropNotNull => self.kw("DROP NOT NULL"),
        }
    }

    fn format_drop(&self, stmt: &DropStatement) -> String {
        let mut parts = vec![self.kw("DROP")];

        let obj_type = match stmt.object_type {
            ObjectType::Table => "TABLE",
            ObjectType::Index => "INDEX",
            ObjectType::Sequence => "SEQUENCE",
            ObjectType::View => "VIEW",
            ObjectType::Schema => "SCHEMA",
            ObjectType::Database => "DATABASE",
            ObjectType::Tablespace => "TABLESPACE",
            ObjectType::Function => "FUNCTION",
            ObjectType::Procedure => "PROCEDURE",
            ObjectType::Trigger => "TRIGGER",
            ObjectType::Extension => "EXTENSION",
            ObjectType::MaterializedView => "MATERIALIZED VIEW",
            ObjectType::ForeignTable => "FOREIGN TABLE",
            ObjectType::ForeignServer => "SERVER",
            ObjectType::Fdw => "FOREIGN DATA WRAPPER",
            ObjectType::Aggregate => "AGGREGATE",
            ObjectType::Cast => "CAST",
            ObjectType::Conversion => "CONVERSION",
            ObjectType::Operator => "OPERATOR",
            ObjectType::OperatorClass => "OPERATOR CLASS",
            ObjectType::OperatorFamily => "OPERATOR FAMILY",
            ObjectType::Rule => "RULE",
            ObjectType::Language => "LANGUAGE",
            ObjectType::TextSearchConfig => "TEXT SEARCH CONFIGURATION",
            ObjectType::TextSearchDict => "TEXT SEARCH DICTIONARY",
            ObjectType::Domain => "DOMAIN",
            ObjectType::Policy => "POLICY",
            ObjectType::User => "USER",
            ObjectType::Role => "ROLE",
            ObjectType::Group => "GROUP",
            ObjectType::ResourcePool => "RESOURCE POOL",
            ObjectType::ResourceLabel => "RESOURCE LABEL",
            ObjectType::WorkloadGroup => "WORKLOAD GROUP",
            ObjectType::AuditPolicy => "AUDIT POLICY",
            ObjectType::MaskingPolicy => "MASKING POLICY",
            ObjectType::RlsPolicy => "ROW LEVEL SECURITY POLICY",
            ObjectType::DataSource => "DATA SOURCE",
            ObjectType::Directory => "DIRECTORY",
            ObjectType::Event => "EVENT",
            ObjectType::Publication => "PUBLICATION",
            ObjectType::Subscription => "SUBSCRIPTION",
            ObjectType::Synonym => "SYNONYM",
            ObjectType::Model => "MODEL",
            ObjectType::SecurityLabel => "SECURITY LABEL",
            ObjectType::UserMapping => "USER MAPPING",
            ObjectType::WeakPasswordDictionary => "WEAK PASSWORD DICTIONARY",
            ObjectType::PolicyLabel => "POLICY LABEL",
            ObjectType::Node => "NODE",
            ObjectType::NodeGroup => "NODE GROUP",
            ObjectType::App => "APP",
            ObjectType::Global => "GLOBAL",
            ObjectType::OpClass => "OPERATOR CLASS",
            ObjectType::OpFamily => "OPERATOR FAMILY",
            ObjectType::Type => "TYPE",
            ObjectType::Package => "PACKAGE",
            ObjectType::DatabaseLink => "DATABASE LINK",
        };
        parts.push(self.kw(obj_type));

        if stmt.if_exists {
            parts.push(self.kw("IF EXISTS"));
        }

        let names: Vec<String> = stmt.names.iter().map(|n| self.format_object_name(n)).collect();
        parts.push(names.join(", "));

        if stmt.cascade {
            parts.push(self.kw("CASCADE"));
        }

        if stmt.purge {
            parts.push(self.kw("PURGE"));
        }

        parts.join(" ")
    }

    fn format_truncate(&self, stmt: &TruncateStatement) -> String {
        let mut parts = vec![self.kw("TRUNCATE TABLE")];

        let tables: Vec<String> = stmt.tables.iter().map(|t| self.format_object_name(t)).collect();
        parts.push(tables.join(", "));

        if stmt.restart_identity {
            parts.push(self.kw("RESTART IDENTITY"));
        }

        if stmt.continue_identity {
            parts.push(self.kw("CONTINUE IDENTITY"));
        }

        if stmt.cascade {
            parts.push(self.kw("CASCADE"));
        }

        parts.join(" ")
    }

    fn format_create_index(&self, stmt: &CreateIndexStatement) -> String {
        let mut parts = vec![self.kw("CREATE")];

        if stmt.unique {
            parts.push(self.kw("UNIQUE"));
        }

        if stmt.unique {
            parts.push(self.kw("UNIQUE"));
        }
        parts.push(self.kw("INDEX"));

        if stmt.if_not_exists {
            parts.push(self.kw("IF NOT EXISTS"));
        }

        if let Some(name) = &stmt.name {
            parts.push(self.format_object_name(name));
        }

        parts.push(self.kw("ON"));
        parts.push(self.format_object_name(&stmt.table));

        if let Some(method) = &stmt.using_method {
            parts.push(self.kw("USING"));
            parts.push(method.clone());
        }

        let cols: Vec<String> = stmt
            .columns
            .iter()
            .map(|c| {
                let mut result = if let Some(name) = &c.name {
                    self.quote_identifier(name)
                } else if let Some(expr) = &c.expr {
                    self.format_expr(expr)
                } else {
                    String::new()
                };
                if let Some(collation) = &c.collation {
                    result = format!("{} {} {}", result, self.kw("COLLATE"), collation);
                }
                if let Some(opclass) = &c.opclass {
                    result = format!("{} {}", result, opclass);
                }
                if let Some(asc) = c.asc {
                    result = format!("{} {}", result, if asc { "ASC" } else { "DESC" });
                }
                if let Some(nulls) = &c.nulls {
                    result = format!(
                        "{} {} {}",
                        result,
                        self.kw("NULLS"),
                        match nulls {
                            IndexNulls::First => "FIRST",
                            IndexNulls::Last => "LAST",
                        }
                    );
                }
                result
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));

        if let Some(where_clause) = &stmt.where_clause {
            parts.push(format!("{} {}", self.kw("WHERE"), self.format_expr(where_clause)));
        }

        parts.join(" ")
    }

    fn format_create_global_index(&self, stmt: &CreateGlobalIndexStatement) -> String {
        let mut parts = vec![self.kw("CREATE"), self.kw("GLOBAL")];

        if stmt.unique {
            parts.push(self.kw("UNIQUE"));
        }

        parts.push(self.kw("INDEX"));

        if stmt.concurrent {
            parts.push(self.kw("CONCURRENTLY"));
        }

        if stmt.if_not_exists {
            parts.push(self.kw("IF NOT EXISTS"));
        }

        if let Some(name) = &stmt.name {
            parts.push(self.format_object_name(name));
        }

        parts.push(self.kw("ON"));
        parts.push(self.format_object_name(&stmt.table));

        if let Some(method) = &stmt.using_method {
            parts.push(self.kw("USING"));
            parts.push(method.clone());
        }

        let cols: Vec<String> = stmt
            .columns
            .iter()
            .map(|c| {
                let mut result = if let Some(expr) = &c.expression {
                    format!("({})", self.format_expr(expr))
                } else {
                    let mut r = self.quote_identifier(&c.name);
                    if let Some(len) = c.length {
                        r = format!("{}({})", r, len);
                    }
                    if let Some(coll) = &c.collation {
                        r = format!("{} {} {}", r, self.kw("COLLATE"), coll);
                    }
                    if let Some(opc) = &c.opclass {
                        r = format!("{} {}", r, opc);
                    }
                    r
                };
                if let Some(ord) = &c.ordering {
                    result = format!(
                        "{} {}",
                        result,
                        match ord {
                            IndexOrdering::Asc => self.kw("ASC"),
                            IndexOrdering::Desc => self.kw("DESC"),
                        }
                    );
                }
                if let Some(n) = &c.nulls {
                    result = format!(
                        "{} {} {}",
                        result,
                        self.kw("NULLS"),
                        match n {
                            IndexNulls::First => self.kw("FIRST"),
                            IndexNulls::Last => self.kw("LAST"),
                        }
                    );
                }
                result
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));

        if !stmt.containing.is_empty() {
            parts.push(format!(
                "CONTAINING ({})",
                stmt.containing.iter().map(|c| self.quote_identifier(c)).collect::<Vec<_>>().join(", ")
            ));
        }

        if let Some(dist) = &stmt.distribute_by {
            parts.push(self.format_distribute_clause(dist));
        }

        if !stmt.with_options.is_empty() {
            let opts: Vec<String> = stmt.with_options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
            parts.push(format!("{} ({})", self.kw("WITH"), opts.join(", ")));
        }

        if let Some(ts) = &stmt.tablespace {
            parts.push(format!("{} {}", self.kw("TABLESPACE"), self.quote_identifier(ts)));
        }

        if let Some(vis) = stmt.visible {
            parts.push(if vis { self.kw("VISIBLE") } else { self.kw("INVISIBLE") });
        }

        if let Some(where_clause) = &stmt.where_clause {
            parts.push(format!("{} {}", self.kw("WHERE"), self.format_expr(where_clause)));
        }

        parts.join(" ")
    }

    fn format_create_view(&self, stmt: &CreateViewStatement) -> String {
        let mut parts = vec![self.kw("CREATE")];

        if stmt.replace {
            parts.push(self.kw("OR REPLACE"));
        }

        if stmt.temporary {
            parts.push(self.kw("TEMPORARY"));
        }

        if stmt.recursive {
            parts.push(self.kw("RECURSIVE"));
        }

        parts.push(self.kw("VIEW"));
        parts.push(self.format_object_name(&stmt.name));

        if !stmt.columns.is_empty() {
            parts.push(format!("({})", stmt.columns.join(", ")));
        }

        parts.push(self.kw("AS"));
        parts.push(self.format_select(&stmt.query));

        if let Some(check) = &stmt.check_option {
            parts.push(self.kw("WITH"));
            match check {
                CheckOption::Local => parts.push(self.kw("LOCAL CHECK OPTION")),
                CheckOption::Cascaded => parts.push(self.kw("CASCADED CHECK OPTION")),
            }
        }

        if let Some(security) = &stmt.security {
            match security {
                ViewSecurity::Barrier => parts.push(self.kw("SECURITY BARRIER")),
                ViewSecurity::Invoker => parts.push(self.kw("SECURITY INVOKER")),
            }
        }

        parts.join(" ")
    }

    fn format_create_sequence(&self, stmt: &CreateSequenceStatement) -> String {
        let mut parts = vec![self.kw("CREATE SEQUENCE")];

        if stmt.if_not_exists {
            parts.push(self.kw("IF NOT EXISTS"));
        }

        parts.push(self.format_object_name(&stmt.name));

        if let Some(start) = &stmt.start {
            parts.push(format!("{} {}", self.kw("START WITH"), self.format_expr(start)));
        }

        if let Some(incr) = &stmt.increment {
            parts.push(format!("{} {}", self.kw("INCREMENT BY"), self.format_expr(incr)));
        }

        if let Some(min) = &stmt.min_value {
            parts.push(format!("{} {}", self.kw("MINVALUE"), self.format_expr(min)));
        }

        if let Some(max) = &stmt.max_value {
            parts.push(format!("{} {}", self.kw("MAXVALUE"), self.format_expr(max)));
        }

        if let Some(cache) = &stmt.cache {
            parts.push(format!("{} {}", self.kw("CACHE"), self.format_expr(cache)));
        }

        if stmt.cycle {
            parts.push(self.kw("CYCLE"));
        }

        if let Some(owned) = &stmt.owned_by {
            parts.push(format!("{} {} {}", self.kw("OWNED"), self.kw("BY"), self.format_object_name(owned)));
        }

        parts.join(" ")
    }

    fn format_create_schema(&self, stmt: &CreateSchemaStatement) -> String {
        let mut parts = vec![self.kw("CREATE SCHEMA")];

        if stmt.if_not_exists {
            parts.push(self.kw("IF NOT EXISTS"));
        }

        if let Some(name) = &stmt.name {
            parts.push(self.quote_identifier(name));
        }

        if let Some(auth) = &stmt.authorization {
            parts.push(self.kw("AUTHORIZATION"));
            parts.push(self.quote_identifier(auth));
        }

        if let Some(cs) = &stmt.character_set {
            parts.push(self.kw("CHARACTER SET"));
            parts.push(self.quote_identifier(cs));
        }

        if let Some(coll) = &stmt.collate {
            parts.push(self.kw("COLLATE"));
            parts.push(self.quote_identifier(coll));
        }

        parts.join(" ")
    }

    fn format_create_database(&self, stmt: &CreateDatabaseStatement) -> String {
        let mut parts = vec![self.kw("CREATE DATABASE")];
        parts.push(self.quote_identifier(&stmt.name));

        if stmt.owner.is_some()
            || stmt.template.is_some()
            || stmt.encoding.is_some()
            || stmt.locale.is_some()
            || stmt.lc_collate.is_some()
            || stmt.lc_ctype.is_some()
            || stmt.tablespace.is_some()
            || stmt.allow_connections.is_some()
            || stmt.connection_limit.is_some()
            || stmt.is_template.is_some()
        {
            parts.push(self.kw("WITH"));
        }

        if let Some(owner) = &stmt.owner {
            parts.push(format!("{} = {}", self.kw("OWNER"), self.quote_identifier(owner)));
        }

        if let Some(template) = &stmt.template {
            parts.push(format!("{} = {}", self.kw("TEMPLATE"), self.quote_identifier(template)));
        }

        if let Some(encoding) = &stmt.encoding {
            parts.push(format!("{} = {}", self.kw("ENCODING"), self.quote_string(encoding)));
        }

        if let Some(locale) = &stmt.locale {
            parts.push(format!("{} = {}", self.kw("LOCALE"), self.quote_string(locale)));
        }

        if let Some(lc_collate) = &stmt.lc_collate {
            parts.push(format!("{} = {}", self.kw("LC_COLLATE"), self.quote_string(lc_collate)));
        }

        if let Some(lc_ctype) = &stmt.lc_ctype {
            parts.push(format!("{} = {}", self.kw("LC_CTYPE"), self.quote_string(lc_ctype)));
        }

        if let Some(tablespace) = &stmt.tablespace {
            parts.push(format!("{} = {}", self.kw("TABLESPACE"), self.quote_identifier(tablespace)));
        }

        if let Some(allow_conn) = &stmt.allow_connections {
            parts.push(format!("{} = {}", self.kw("ALLOW_CONNECTIONS"), allow_conn));
        }

        if let Some(conn_limit) = &stmt.connection_limit {
            parts.push(format!("{} = {}", self.kw("CONNECTION LIMIT"), conn_limit));
        }

        if let Some(is_template) = &stmt.is_template {
            parts.push(format!("{} = {}", self.kw("IS_TEMPLATE"), is_template));
        }

        parts.join(" ")
    }

    fn format_create_database_link(&self, stmt: &CreateDatabaseLinkStatement) -> String {
        let mut parts = vec![self.kw("CREATE")];
        if stmt.public_link {
            parts.push(self.kw("PUBLIC"));
        }
        parts.push(self.kw("DATABASE LINK"));
        parts.push(self.quote_identifier(&stmt.name));
        if let Some(user) = &stmt.user {
            parts.push(format!("{} {}", self.kw("CONNECT TO"), self.quote_identifier(user)));
        }
        if let Some(pwd) = &stmt.password {
            parts.push(format!("{} {}", self.kw("IDENTIFIED BY"), self.quote_string(pwd)));
        }
        if let Some(using) = &stmt.using_clause {
            parts.push(format!("{} {}", self.kw("USING"), self.quote_string(using)));
        }
        parts.join(" ")
    }

    fn format_create_tablespace(&self, stmt: &CreateTablespaceStatement) -> String {
        let mut parts = vec![self.kw("CREATE TABLESPACE")];
        parts.push(self.quote_identifier(&stmt.name));

        if let Some(owner) = &stmt.owner {
            parts.push(self.kw("OWNER"));
            parts.push(self.quote_identifier(owner));
        }

        if stmt.relative {
            parts.push(self.kw("RELATIVE"));
        }

        parts.push(self.kw("LOCATION"));
        parts.push(self.quote_string(&stmt.location));

        if let Some(maxsize) = &stmt.maxsize {
            parts.push(self.kw("MAXSIZE"));
            parts.push(self.quote_string(maxsize));
        }

        parts.join(" ")
    }

    fn format_create_conversion(&self, stmt: &CreateConversionStatement) -> String {
        let mut parts = vec![self.kw("CREATE CONVERSION")];
        parts.push(self.quote_identifier(&stmt.name));
        parts.push(self.kw("FOR"));
        parts.push(self.quote_identifier(&stmt.source_encoding));
        parts.push(self.kw("TO"));
        parts.push(self.quote_identifier(&stmt.dest_encoding));
        parts.push(self.kw("FROM"));
        parts.push(self.quote_identifier(&stmt.function_name));
        parts.join(" ")
    }

    fn format_create_synonym(&self, stmt: &CreateSynonymStatement) -> String {
        let mut parts = vec![self.kw("CREATE")];
        if stmt.replace {
            parts.push(self.kw("OR REPLACE"));
        }
        parts.push(self.kw("SYNONYM"));
        parts.push(self.format_object_name(&stmt.name));
        parts.push(self.kw("FOR"));
        parts.push(self.format_object_name(&stmt.target));
        if stmt.public {
            parts.push(self.kw("PUBLIC"));
        }
        parts.join(" ")
    }

    fn format_create_model(&self, stmt: &CreateModelStatement) -> String {
        let mut parts = vec![self.kw("CREATE MODEL")];
        parts.push(self.quote_identifier(&stmt.name));
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_create_am(&self, stmt: &CreateAmStatement) -> String {
        let mut parts = vec![self.kw("CREATE ACCESS METHOD")];
        parts.push(self.quote_identifier(&stmt.name));
        parts.push(self.kw("TYPE"));
        parts.push(self.quote_identifier(&stmt.method));
        parts.push(self.kw("HANDLER"));
        parts.push(self.quote_identifier(&stmt.handler));
        parts.join(" ")
    }

    fn format_create_directory(&self, stmt: &CreateDirectoryStatement) -> String {
        let mut parts = vec![self.kw("CREATE DIRECTORY")];
        parts.push(self.quote_identifier(&stmt.name));
        if !stmt.path.is_empty() {
            parts.push(self.kw("AS"));
            parts.push(self.quote_string(&stmt.path));
        }
        parts.join(" ")
    }

    fn format_create_data_source(&self, stmt: &CreateDataSourceStatement) -> String {
        let mut parts = vec![self.kw("CREATE DATA SOURCE")];
        parts.push(self.quote_identifier(&stmt.name));
        if let Some(ref t) = stmt.ds_type {
            parts.push(format!("{} {}", self.kw("TYPE"), self.quote_string(t)));
        }
        if let Some(ref v) = stmt.version {
            parts.push(format!("{} {}", self.kw("VERSION"), v));
        }
        if !stmt.options.is_empty() {
            parts.push(self.kw("WITH"));
            parts.push("(".to_string());
            let opts: Vec<String> = stmt
                .options
                .iter()
                .map(|(k, v)| format!("{} = {}", self.quote_identifier(k), self.quote_string(v)))
                .collect();
            parts.push(opts.join(", "));
            parts.push(")".to_string());
        }
        parts.join(" ")
    }

    fn format_create_event(&self, stmt: &CreateEventStatement) -> String {
        let mut parts = vec![self.kw("CREATE EVENT")];
        parts.push(self.quote_identifier(&stmt.name));
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_create_opclass(&self, stmt: &CreateOpClassStatement) -> String {
        let mut parts = vec![self.kw("CREATE OPERATOR CLASS")];
        parts.push(self.quote_identifier(&stmt.name));
        parts.push(self.kw("USING"));
        parts.push(self.quote_identifier(&stmt.method));
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_create_opfamily(&self, stmt: &CreateOpFamilyStatement) -> String {
        let mut parts = vec![self.kw("CREATE OPERATOR FAMILY")];
        parts.push(self.quote_identifier(&stmt.name));
        parts.push(self.kw("USING"));
        parts.push(self.quote_identifier(&stmt.method));
        parts.join(" ")
    }

    fn format_create_contquery(&self, stmt: &CreateContQueryStatement) -> String {
        let mut parts = vec![self.kw("CREATE CONTINUOUS QUERY")];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_create_stream(&self, stmt: &CreateStreamStatement) -> String {
        let mut parts = vec![self.kw("CREATE STREAM")];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_create_key(&self, stmt: &CreateKeyStatement) -> String {
        let mut parts = vec![self.kw("CREATE KEY")];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_foreign_table(&self, stmt: &AlterForeignTableStatement) -> String {
        let mut parts = vec![self.kw("ALTER FOREIGN TABLE"), self.format_object_name(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_foreign_server(&self, stmt: &AlterForeignServerStatement) -> String {
        let mut parts = vec![self.kw("ALTER SERVER"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_fdw(&self, stmt: &AlterFdwStatement) -> String {
        let mut parts = vec![self.kw("ALTER FOREIGN DATA WRAPPER"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_publication(&self, stmt: &AlterPublicationStatement) -> String {
        let mut parts = vec![self.kw("ALTER PUBLICATION"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_subscription(&self, stmt: &AlterSubscriptionStatement) -> String {
        let mut parts = vec![self.kw("ALTER SUBSCRIPTION"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_node(&self, stmt: &AlterNodeStatement) -> String {
        let mut parts = vec![self.kw("ALTER NODE"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_node_group(&self, stmt: &AlterNodeGroupStatement) -> String {
        let mut parts = vec![self.kw("ALTER NODE GROUP"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_workload_group(&self, stmt: &AlterWorkloadGroupStatement) -> String {
        let mut parts = vec![self.kw("ALTER WORKLOAD GROUP"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_audit_policy(&self, stmt: &AlterAuditPolicyStatement) -> String {
        let mut parts = vec![self.kw("ALTER AUDIT POLICY"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_rls_policy(&self, stmt: &AlterRlsPolicyStatement) -> String {
        let mut parts = vec![self.kw("ALTER RLS POLICY"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_data_source(&self, stmt: &AlterDataSourceStatement) -> String {
        let mut parts = vec![self.kw("ALTER DATA SOURCE"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_event(&self, stmt: &AlterEventStatement) -> String {
        let mut parts = vec![self.kw("ALTER EVENT"), self.quote_identifier(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_opfamily(&self, stmt: &AlterOpFamilyStatement) -> String {
        let mut parts = vec![
            self.kw("ALTER OPERATOR FAMILY"),
            self.quote_identifier(&stmt.name),
            self.kw("USING"),
            self.quote_identifier(&stmt.method),
        ];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_alter_materialized_view(&self, stmt: &AlterMaterializedViewStatement) -> String {
        let mut parts = vec![self.kw("ALTER MATERIALIZED VIEW"), self.format_object_name(&stmt.name)];
        if !stmt.raw_rest.is_empty() {
            parts.push(stmt.raw_rest.clone());
        }
        parts.join(" ")
    }

    fn format_transaction(&self, stmt: &TransactionStatement) -> String {
        match &stmt.kind {
            TransactionKind::Begin => {
                let mut parts = vec![self.kw("BEGIN")];
                if !stmt.modes.is_empty() {
                    parts.push(self.format_transaction_modes(&stmt.modes));
                }
                parts.join(" ")
            }
            TransactionKind::Commit => self.kw("COMMIT"),
            TransactionKind::Rollback => self.kw("ROLLBACK"),
            TransactionKind::Savepoint => {
                format!(
                    "{} {}",
                    self.kw("SAVEPOINT"),
                    self.quote_identifier(stmt.savepoint_name.as_ref().expect("Savepoint must have savepoint_name"))
                )
            }
            TransactionKind::ReleaseSavepoint => {
                format!(
                    "{} {} {}",
                    self.kw("RELEASE"),
                    self.kw("SAVEPOINT"),
                    self.quote_identifier(
                        stmt.savepoint_name.as_ref().expect("ReleaseSavepoint must have savepoint_name")
                    )
                )
            }
            TransactionKind::PrepareTransaction => {
                format!(
                    "{} {} '{}'",
                    self.kw("PREPARE"),
                    self.kw("TRANSACTION"),
                    stmt.transaction_id.as_ref().expect("PrepareTransaction must have transaction_id")
                )
            }
            TransactionKind::CommitPrepared => {
                format!(
                    "{} {} '{}'",
                    self.kw("COMMIT"),
                    self.kw("PREPARED"),
                    stmt.transaction_id.as_ref().expect("CommitPrepared must have transaction_id")
                )
            }
            TransactionKind::RollbackPrepared => {
                format!(
                    "{} {} '{}'",
                    self.kw("ROLLBACK"),
                    self.kw("PREPARED"),
                    stmt.transaction_id.as_ref().expect("RollbackPrepared must have transaction_id")
                )
            }
            TransactionKind::SetTransaction => {
                let mut parts = vec![self.kw("SET"), self.kw("TRANSACTION")];
                if !stmt.modes.is_empty() {
                    parts.push(self.format_transaction_modes(&stmt.modes));
                }
                parts.join(" ")
            }
        }
    }

    fn format_transaction_modes(&self, modes: &[TransactionMode]) -> String {
        let formatted: Vec<String> = modes.iter().map(|m| self.format_transaction_mode(m)).collect();
        formatted.join(", ")
    }

    fn format_transaction_mode(&self, mode: &TransactionMode) -> String {
        match mode {
            TransactionMode::IsolationLevel(level) => {
                let level_str = match level {
                    IsolationLevel::ReadUncommitted => "READ UNCOMMITTED",
                    IsolationLevel::ReadCommitted => "READ COMMITTED",
                    IsolationLevel::RepeatableRead => "REPEATABLE READ",
                    IsolationLevel::Serializable => "SERIALIZABLE",
                };
                format!("{} {}", self.kw("ISOLATION LEVEL"), self.kw(level_str))
            }
            TransactionMode::ReadOnly => self.kw("READ ONLY"),
            TransactionMode::ReadWrite => self.kw("READ WRITE"),
            TransactionMode::Deferrable => self.kw("DEFERRABLE"),
            TransactionMode::NotDeferrable => self.kw("NOT DEFERRABLE"),
        }
    }

    fn format_variable_set(&self, stmt: &VariableSetStatement) -> String {
        let mut parts = vec![self.kw("SET")];

        if stmt.session {
            parts.push(self.kw("SESSION"));
        } else if stmt.local {
            parts.push(self.kw("LOCAL"));
        } else if stmt.global {
            parts.push(self.kw("GLOBAL"));
        }

        parts.push(stmt.name.clone());
        parts.push("=".to_string());

        if stmt.value.len() == 1 {
            parts.push(self.format_expr(&stmt.value[0]));
        } else if !stmt.value.is_empty() {
            parts.push(format!("({})", self.format_exprs(&stmt.value)));
        }

        parts.join(" ")
    }

    fn format_variable_show(&self, stmt: &VariableShowStatement) -> String {
        let mut s = format!("{} {}", self.kw("SHOW"), stmt.name);
        if let Some(ref pattern) = stmt.like_pattern {
            s = format!("{} {} {}", s, self.kw("LIKE"), pattern);
        }
        s
    }

    fn format_variable_reset(&self, stmt: &VariableResetStatement) -> String {
        format!("{} {}", self.kw("RESET"), stmt.name)
    }

    fn format_discard(&self, stmt: &DiscardStatement) -> String {
        let target = match stmt.target {
            DiscardTarget::All => "ALL",
            DiscardTarget::Plans => "PLANS",
            DiscardTarget::Sequences => "SEQUENCES",
            DiscardTarget::Temp => "TEMP",
        };
        format!("{} {}", self.kw("DISCARD"), self.kw(target))
    }

    fn format_copy(&self, stmt: &CopyStatement) -> String {
        let mut parts = vec![self.kw("COPY")];

        if let Some(query) = &stmt.query {
            parts.push(format!("({})", self.format_select(query)));
        } else if let Some(relation) = &stmt.relation {
            parts.push(self.format_object_name(relation));
            if !stmt.columns.is_empty() {
                parts.push(format!("({})", stmt.columns.join(", ")));
            }
        }

        if stmt.is_from {
            parts.push(self.kw("FROM"));
        } else {
            parts.push(self.kw("TO"));
        }

        if let Some(filename) = &stmt.filename {
            parts.push(self.quote_string(filename));
        } else if stmt.is_from {
            parts.push(self.kw("STDIN"));
        } else {
            parts.push(self.kw("STDOUT"));
        }

        if !stmt.options.is_empty() {
            let opts: Vec<String> = stmt
                .options
                .iter()
                .map(|o| match &o.value {
                    Some(v) => format!("{} {}", o.name, v),
                    None => o.name.clone(),
                })
                .collect();
            parts.push(format!("{} ({})", self.kw("WITH"), opts.join(", ")));
        }

        parts.join(" ")
    }

    fn format_explain(&self, stmt: &ExplainStatement) -> String {
        let mut parts = vec![self.kw("EXPLAIN")];

        if stmt.analyze {
            parts.push(self.kw("ANALYZE"));
        }

        if stmt.verbose {
            parts.push(self.kw("VERBOSE"));
        }

        if stmt.performance {
            parts.push(self.kw("PERFORMANCE"));
        }

        if stmt.plan {
            parts.push(self.kw("PLAN"));
            if let Some(ref sid) = stmt.statement_id {
                parts.push(format!("{} {} {} '{}'", self.kw("SET"), self.kw("STATEMENT_ID"), "=", sid));
            }
            parts.push(self.kw("FOR"));
        }

        if !stmt.options.is_empty() {
            let opts: Vec<String> = stmt
                .options
                .iter()
                .map(|o| match &o.value {
                    Some(v) => format!("{} {}", o.name, v),
                    None => o.name.clone(),
                })
                .collect();
            parts.push(format!("({})", opts.join(", ")));
        }

        parts.push(self.format_statement(&stmt.query));

        parts.join(" ")
    }

    fn format_call(&self, stmt: &CallFuncStatement) -> String {
        let args: Vec<String> = stmt.args.iter().map(|a| self.format_call_arg(a)).collect();
        format!("{} {}({})", self.kw("CALL"), self.format_object_name(&stmt.func_name), args.join(", "))
    }

    fn format_call_arg(&self, arg: &CallArg) -> String {
        match arg {
            CallArg::Positional(expr) => self.format_expr(expr),
            CallArg::Named { name, arg, uses_arrow } => {
                if *uses_arrow {
                    format!("{} => {}", name, self.format_expr(arg))
                } else {
                    format!("{} = {}", name, self.format_expr(arg))
                }
            }
        }
    }
}

impl Default for SqlFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Statement {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", SqlFormatter::new().format_statement(self))
    }
}

impl SqlFormatter {
    fn format_options(&self, options: &[(String, String)]) -> String {
        if options.is_empty() {
            return String::new();
        }
        let pairs: Vec<String> = options.iter().map(|(k, v)| format!("{} = '{}'", k, v)).collect();
        format!(" WITH ({})", pairs.join(", "))
    }

    fn format_create_fdw(&self, stmt: &CreateFdwStatement) -> String {
        let mut s = format!("CREATE FOREIGN DATA WRAPPER {}", stmt.name);
        if let Some(ref h) = stmt.handler {
            s.push_str(&format!(" HANDLER {}", h));
        }
        if let Some(ref v) = stmt.validator {
            s.push_str(&format!(" VALIDATOR {}", v));
        }
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_foreign_server(&self, stmt: &CreateForeignServerStatement) -> String {
        let mut s = format!("CREATE SERVER {}", stmt.name);
        if let Some(ref t) = stmt.server_type {
            s.push_str(&format!(" TYPE '{}'", t));
        }
        if let Some(ref v) = stmt.version {
            s.push_str(&format!(" VERSION '{}'", v));
        }
        s.push_str(&format!(" FOREIGN DATA WRAPPER {}", stmt.fdw_name));
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_foreign_table(&self, stmt: &CreateForeignTableStatement) -> String {
        let mut s = format!("CREATE FOREIGN TABLE {}", self.format_object_name(&stmt.name));
        if !stmt.columns.is_empty() {
            let cols: Vec<String> = stmt.columns.iter().map(|c| self.format_column_def(c)).collect();
            s.push_str(&format!(" ({})", cols.join(", ")));
        }
        s.push_str(&format!(" SERVER {}", stmt.server_name));
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_publication(&self, stmt: &CreatePublicationStatement) -> String {
        let mut s = format!("CREATE PUBLICATION {}", stmt.name);
        if stmt.all_tables {
            s.push_str(" FOR ALL TABLES");
        } else if !stmt.tables.is_empty() {
            let tables: Vec<String> = stmt.tables.iter().map(|t| self.format_object_name(t)).collect();
            s.push_str(&format!(" FOR TABLE {}", tables.join(", ")));
        }
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_subscription(&self, stmt: &CreateSubscriptionStatement) -> String {
        let mut s = format!("CREATE SUBSCRIPTION {}", stmt.name);
        s.push_str(&format!(" CONNECTION '{}'", stmt.connection));
        let pubs = stmt.publications.join(", ");
        s.push_str(&format!(" PUBLICATION {}", pubs));
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_node(&self, stmt: &CreateNodeStatement) -> String {
        let mut s = format!("CREATE NODE {}", stmt.name);
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_node_group(&self, stmt: &CreateNodeGroupStatement) -> String {
        let mut s = format!("CREATE NODE GROUP {}", stmt.name);
        if !stmt.nodes.is_empty() {
            s.push_str(&format!(" ({})", stmt.nodes.join(", ")));
        }
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_resource_pool(&self, stmt: &CreateResourcePoolStatement) -> String {
        let mut s = format!("CREATE RESOURCE POOL {}", stmt.name);
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_workload_group(&self, stmt: &CreateWorkloadGroupStatement) -> String {
        let mut s = format!("CREATE WORKLOAD GROUP {}", stmt.name);
        if let Some(ref p) = stmt.pool_name {
            s.push_str(&format!(" USING RESOURCE POOL {}", p));
        }
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_audit_policy(&self, stmt: &CreateAuditPolicyStatement) -> String {
        let mut s = format!("CREATE AUDIT POLICY {}", stmt.name);
        s.push_str(&format!(" {}", stmt.policy_type));
        if !stmt.privileges.is_empty() {
            s.push(' ');
            s.push_str(&stmt.privileges.join(", "));
        }
        if !stmt.labels.is_empty() {
            s.push_str(&format!(" ON LABEL ({})", stmt.labels.join(", ")));
        }
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_filter_clauses(&self, clauses: &[FilterClause]) -> String {
        let parts: Vec<String> = clauses.iter().map(|c| format!("{} ({})", c.kind, c.values.join(", "))).collect();
        parts.join(", ")
    }

    fn format_create_masking_policy(&self, stmt: &CreateMaskingPolicyStatement) -> String {
        let mut s = format!("CREATE MASKING POLICY {}", stmt.name);
        if let Some(ref func) = stmt.masking_function {
            s.push_str(&format!(" {}", func));
        }
        if !stmt.function_args.is_empty() {
            let args: Vec<String> = stmt.function_args.iter().map(|a| self.format_expr(a)).collect();
            s.push_str(&format!(" ({})", args.join(", ")));
        }
        if !stmt.labels.is_empty() {
            s.push_str(&format!(" ON LABEL ({})", stmt.labels.join(", ")));
        }
        if !stmt.filter_clauses.is_empty() {
            s.push_str(&format!(" FILTER ON {}", self.format_filter_clauses(&stmt.filter_clauses)));
        }
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_create_policy_label(&self, stmt: &CreatePolicyLabelStatement) -> String {
        let op = if stmt.add { "ADD" } else { "REMOVE" };
        let targets: Vec<String> = stmt.targets.iter().map(|t| self.format_object_name(t)).collect();
        let mut s = "CREATE RESOURCE LABEL".to_string();
        if stmt.if_not_exists {
            s.push_str(" IF NOT EXISTS");
        }
        s.push_str(&format!(" {} {} {} ({})", stmt.name, op, stmt.label_type, targets.join(", ")));
        s
    }

    fn format_alter_masking_policy(&self, stmt: &AlterMaskingPolicyStatement) -> String {
        let mut s = format!("ALTER MASKING POLICY {}", stmt.name);
        match &stmt.action {
            AlterMaskingPolicyAction::Comments(comment) => {
                s.push_str(&format!(" COMMENTS '{}'", comment));
            }
            AlterMaskingPolicyAction::Add { function, labels } => {
                s.push_str(&format!(" ADD {} ON LABEL ({})", function, labels.join(", ")));
            }
            AlterMaskingPolicyAction::Remove { function, labels } => {
                s.push_str(&format!(" REMOVE {} ON LABEL ({})", function, labels.join(", ")));
            }
            AlterMaskingPolicyAction::Modify { function, labels } => {
                s.push_str(&format!(" MODIFY {} ON LABEL ({})", function, labels.join(", ")));
            }
            AlterMaskingPolicyAction::ModifyFilter { filter_clauses } => {
                s.push_str(&format!(" MODIFY ( FILTER ON {} )", self.format_filter_clauses(filter_clauses)));
            }
            AlterMaskingPolicyAction::DropFilter => {
                s.push_str(" DROP FILTER");
            }
            AlterMaskingPolicyAction::Disable => {
                s.push_str(" DISABLE");
            }
        }
        s
    }

    fn format_alter_policy_label(&self, stmt: &AlterPolicyLabelStatement) -> String {
        let op = if stmt.add { "ADD" } else { "REMOVE" };
        let targets: Vec<String> = stmt.targets.iter().map(|t| self.format_object_name(t)).collect();
        format!("ALTER RESOURCE LABEL {} {} {} ({})", stmt.name, op, stmt.label_type, targets.join(", "))
    }

    fn format_create_rls_policy(&self, stmt: &CreateRlsPolicyStatement) -> String {
        let mut s = format!("CREATE POLICY {} ON {}", stmt.name, self.format_object_name(&stmt.table));
        if !stmt.permissive {
            s.push_str(" AS RESTRICTIVE");
        }
        if let Some(ref expr) = stmt.using_expr {
            s.push_str(&format!(" USING ({})", self.format_expr(expr)));
        }
        s
    }

    fn format_privilege(&self, p: &Privilege) -> String {
        match p {
            Privilege::All => self.kw("ALL PRIVILEGES"),
            Privilege::Select => self.kw("SELECT"),
            Privilege::SelectColumns(cols) => {
                let cols_str: Vec<String> = cols.iter().map(|c| self.quote_identifier(c)).collect();
                format!("{} ({})", self.kw("SELECT"), cols_str.join(", "))
            }
            Privilege::Insert => self.kw("INSERT"),
            Privilege::Update => self.kw("UPDATE"),
            Privilege::UpdateColumns(cols) => {
                let cols_str: Vec<String> = cols.iter().map(|c| self.quote_identifier(c)).collect();
                format!("{} ({})", self.kw("UPDATE"), cols_str.join(", "))
            }
            Privilege::Delete => self.kw("DELETE"),
            Privilege::Usage => self.kw("USAGE"),
            Privilege::Create => self.kw("CREATE"),
            Privilege::Connect => self.kw("CONNECT"),
            Privilege::Temporary => self.kw("TEMPORARY"),
            Privilege::Execute => self.kw("EXECUTE"),
            Privilege::Trigger => self.kw("TRIGGER"),
            Privilege::References => self.kw("REFERENCES"),
            Privilege::Alter => self.kw("ALTER"),
            Privilege::Drop => self.kw("DROP"),
            Privilege::Comment => self.kw("COMMENT"),
            Privilege::Index => self.kw("INDEX"),
            Privilege::Vacuum => self.kw("VACUUM"),
        }
    }

    fn format_grant(&self, stmt: &GrantStatement) -> String {
        let privs = stmt.privileges.iter().map(|p| self.format_privilege(p)).collect::<Vec<_>>().join(", ");
        let target = match &stmt.target {
            GrantTarget::Table(tables) => {
                format!("TABLE {}", tables.iter().map(|t| self.format_object_name(t)).collect::<Vec<_>>().join(", "))
            }
            GrantTarget::Schema(schemas) => format!("SCHEMA {}", schemas.join(", ")),
            GrantTarget::Database(dbs) => format!("DATABASE {}", dbs.join(", ")),
            GrantTarget::Function(funcs) => {
                format!("FUNCTION {}", funcs.iter().map(|f| self.format_object_name(f)).collect::<Vec<_>>().join(", "))
            }
            GrantTarget::Sequence(seqs) => {
                format!("SEQUENCE {}", seqs.iter().map(|s| self.format_object_name(s)).collect::<Vec<_>>().join(", "))
            }
            GrantTarget::AllTablesInSchema(schemas) => {
                format!("ALL TABLES IN SCHEMA {}", schemas.join(", "))
            }
            GrantTarget::AllFunctionsInSchema(schemas) => {
                format!("ALL FUNCTIONS IN SCHEMA {}", schemas.join(", "))
            }
            GrantTarget::AllSequencesInSchema(schemas) => {
                format!("ALL SEQUENCES IN SCHEMA {}", schemas.join(", "))
            }
            GrantTarget::Tablespace(tbs) => {
                format!("TABLESPACE {}", tbs.join(", "))
            }
            GrantTarget::Language(names) => {
                format!("LANGUAGE {}", names.join(", "))
            }
            GrantTarget::LargeObject(names) => {
                format!("LARGE OBJECT {}", names.join(", "))
            }
            GrantTarget::Type(types) => {
                format!("TYPE {}", types.iter().map(|t| self.format_object_name(t)).collect::<Vec<_>>().join(", "))
            }
        };
        let mut s = format!("{} {} ON {} TO {}", self.kw("GRANT"), privs, target, stmt.grantees.join(", "));
        if stmt.with_grant_option {
            s.push_str(&format!(" {}", self.kw("WITH GRANT OPTION")));
        }
        if let Some(ref by) = stmt.granted_by {
            s.push_str(&format!(" {} {}", self.kw("GRANTED BY"), by));
        }
        s
    }

    fn format_revoke(&self, stmt: &RevokeStatement) -> String {
        let privs = stmt.privileges.iter().map(|p| self.format_privilege(p)).collect::<Vec<_>>().join(", ");
        let target = match &stmt.target {
            GrantTarget::Table(tables) => {
                format!("TABLE {}", tables.iter().map(|t| self.format_object_name(t)).collect::<Vec<_>>().join(", "))
            }
            GrantTarget::Schema(schemas) => format!("SCHEMA {}", schemas.join(", ")),
            GrantTarget::Database(dbs) => format!("DATABASE {}", dbs.join(", ")),
            GrantTarget::Function(funcs) => {
                format!("FUNCTION {}", funcs.iter().map(|f| self.format_object_name(f)).collect::<Vec<_>>().join(", "))
            }
            GrantTarget::Sequence(seqs) => {
                format!("SEQUENCE {}", seqs.iter().map(|s| self.format_object_name(s)).collect::<Vec<_>>().join(", "))
            }
            GrantTarget::AllTablesInSchema(schemas) => {
                format!("ALL TABLES IN SCHEMA {}", schemas.join(", "))
            }
            GrantTarget::AllFunctionsInSchema(schemas) => {
                format!("ALL FUNCTIONS IN SCHEMA {}", schemas.join(", "))
            }
            GrantTarget::AllSequencesInSchema(schemas) => {
                format!("ALL SEQUENCES IN SCHEMA {}", schemas.join(", "))
            }
            GrantTarget::Tablespace(tbs) => {
                format!("TABLESPACE {}", tbs.join(", "))
            }
            GrantTarget::Language(names) => {
                format!("LANGUAGE {}", names.join(", "))
            }
            GrantTarget::LargeObject(names) => {
                format!("LARGE OBJECT {}", names.join(", "))
            }
            GrantTarget::Type(types) => {
                format!("TYPE {}", types.iter().map(|t| self.format_object_name(t)).collect::<Vec<_>>().join(", "))
            }
        };
        let mut s = format!("{} {} ON {} FROM {}", self.kw("REVOKE"), privs, target, stmt.grantees.join(", "));
        if stmt.cascade {
            s.push_str(&format!(" {}", self.kw("CASCADE")));
        }
        if let Some(ref by) = stmt.granted_by {
            s.push_str(&format!(" {} {}", self.kw("GRANTED BY"), by));
        }
        s
    }

    fn format_vacuum(&self, stmt: &VacuumStatement) -> String {
        let mut opts = Vec::new();
        if stmt.full {
            opts.push(self.kw("FULL"));
        }
        if stmt.verbose {
            opts.push(self.kw("VERBOSE"));
        }
        if stmt.analyze {
            opts.push(self.kw("ANALYZE"));
        }
        if stmt.freeze {
            opts.push(self.kw("FREEZE"));
        }
        let mut s = format!(
            "{}{}",
            self.kw("VACUUM"),
            if opts.is_empty() { String::new() } else { format!(" {}", opts.join(" ")) }
        );
        if !stmt.tables.is_empty() {
            s.push(' ');
            s.push_str(
                &stmt
                    .tables
                    .iter()
                    .map(|t| {
                        let mut s = self.format_object_name(&t.name);
                        if !t.columns.is_empty() {
                            s.push_str(&format!("({})", t.columns.join(", ")));
                        }
                        s
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        s
    }

    fn format_analyze(&self, stmt: &AnalyzeStatement) -> String {
        let mut s = self.kw("ANALYZE").to_string();
        if stmt.verbose {
            s.push(' ');
            s.push_str(&self.kw("VERBOSE"));
        }
        if !stmt.tables.is_empty() {
            s.push(' ');
            s.push_str(&stmt.tables.iter().map(|t| self.format_object_name(&t.name)).collect::<Vec<_>>().join(", "));
        }
        if !stmt.options.is_empty() {
            s.push(' ');
            s.push_str(&self.kw("WITH"));
            s.push(' ');
            s.push_str(&stmt.options.join(", "));
        }
        s
    }

    fn format_do(&self, stmt: &DoStatement) -> String {
        let mut s = self.kw("DO").to_string();
        if let Some(ref lang) = stmt.language {
            s.push_str(&format!(" {} {}", self.kw("LANGUAGE"), lang));
        }
        if let Some(ref block) = stmt.block {
            s.push_str(" $$");
            s.push_str(&self.format_pl_block(block, 0));
            s.push_str(" $$");
        } else {
            s.push(' ');
            s.push_str(&stmt.code);
        }
        s
    }

    fn format_prepare(&self, stmt: &PrepareStatement) -> String {
        let mut s = format!("{} {}", self.kw("PREPARE"), stmt.name);
        if !stmt.data_types.is_empty() {
            s.push_str(&format!("({})", stmt.data_types.join(", ")));
        }
        s.push_str(&format!(" {} ", self.kw("AS")));
        if let Some(ref parsed) = stmt.parsed_statement {
            s.push_str(&self.format_statement(parsed));
        } else {
            s.push_str(&stmt.statement);
        }
        s
    }

    fn format_execute(&self, stmt: &ExecuteStatement) -> String {
        let mut s = format!("{} {}", self.kw("EXECUTE"), stmt.name);
        if !stmt.params.is_empty() {
            s.push_str(&format!(
                "({})",
                stmt.params.iter().map(|p| self.format_expr(p)).collect::<Vec<_>>().join(", ")
            ));
        }
        s
    }

    fn format_execute_direct(&self, stmt: &ExecuteDirectStatement) -> String {
        format!(
            "{} {} {} ({}) '{}'",
            self.kw("EXECUTE"),
            self.kw("DIRECT"),
            self.kw("ON"),
            self.quote_identifier(&stmt.node_name),
            stmt.query
        )
    }

    fn format_values(&self, stmt: &ValuesStatement) -> String {
        let rows: Vec<String> = stmt
            .rows
            .iter()
            .map(|row| {
                let exprs: Vec<String> = row.iter().map(|e| self.format_expr(e)).collect();
                format!("({})", exprs.join(", "))
            })
            .collect();
        let mut s = format!("{} {}", self.kw("VALUES"), rows.join(", "));
        if !stmt.order_by.is_empty() {
            let items: Vec<String> = stmt.order_by.iter().map(|o| self.format_order_by_item(o)).collect();
            s.push_str(&format!(" {} {}", self.kw("ORDER"), self.kw("BY")));
            s.push(' ');
            s.push_str(&items.join(", "));
        }
        if let Some(ref limit) = stmt.limit {
            s.push_str(&format!(" {} {}", self.kw("LIMIT"), self.format_expr(limit)));
        }
        if let Some(ref offset) = stmt.offset {
            s.push_str(&format!(" {} {}", self.kw("OFFSET"), self.format_expr(offset)));
        }
        s
    }

    fn format_deallocate(&self, stmt: &DeallocateStatement) -> String {
        if stmt.all {
            self.kw("DEALLOCATE ALL").to_string()
        } else {
            format!("{} {}", self.kw("DEALLOCATE PREPARE"), stmt.name.as_deref().unwrap_or(""))
        }
    }

    fn format_create_language(&self, stmt: &CreateLanguageStatement) -> String {
        let mut r = self.kw("CREATE").to_string();
        if stmt.trusted {
            r.push(' ');
            r.push_str(&self.kw("TRUSTED"));
        }
        r.push(' ');
        r.push_str(&self.kw("LANGUAGE"));
        r.push(' ');
        r.push_str(&self.quote_identifier(&stmt.name));
        if let Some(ref h) = stmt.handler {
            r.push_str(&format!(" {} {}", self.kw("HANDLER"), self.quote_identifier(h)));
        }
        if let Some(ref i) = stmt.inline_func {
            r.push_str(&format!(" {} {}", self.kw("INLINE"), self.quote_identifier(i)));
        }
        if let Some(ref v) = stmt.validator {
            r.push_str(&format!(" {} {}", self.kw("VALIDATOR"), self.quote_identifier(v)));
        }
        r
    }

    fn format_comment(&self, stmt: &CommentStatement) -> String {
        format!(
            "{} {} {} {} '{}'",
            self.kw("COMMENT ON"),
            stmt.object_type,
            self.format_object_name(&stmt.name),
            self.kw("IS"),
            stmt.comment
        )
    }

    fn format_lock(&self, stmt: &LockStatement) -> String {
        let mut s = format!(
            "{} {}",
            self.kw("LOCK TABLE"),
            stmt.tables.iter().map(|t| self.format_object_name(t)).collect::<Vec<_>>().join(", ")
        );
        if !stmt.mode.is_empty() {
            s.push_str(&format!(" {} {} {}", self.kw("IN"), stmt.mode, self.kw("MODE")));
        }
        if stmt.nowait {
            s.push(' ');
            s.push_str(&self.kw("NOWAIT"));
        }
        s
    }

    fn format_declare_cursor(&self, stmt: &DeclareCursorStatement) -> String {
        let mut s = format!("{} {}", self.kw("DECLARE"), stmt.name);
        if stmt.binary {
            s.push(' ');
            s.push_str(&self.kw("BINARY"));
        }
        match stmt.sensitivity {
            CursorSensitivity::Insensitive => {
                s.push(' ');
                s.push_str(&self.kw("INSENSITIVE"));
            }
            CursorSensitivity::Asensitive => {
                s.push(' ');
                s.push_str(&self.kw("ASENSITIVE"));
            }
            CursorSensitivity::Sensitive => {}
        }
        match stmt.scrollability {
            CursorScrollability::Scroll => {
                s.push(' ');
                s.push_str(&self.kw("SCROLL"));
            }
            CursorScrollability::NoScroll => {
                s.push_str(&format!(" {} {}", self.kw("NO"), self.kw("SCROLL")));
            }
            CursorScrollability::Default => {}
        }
        match stmt.holdability {
            CursorHoldability::WithHold => {
                s.push_str(&format!(" {} {}", self.kw("WITH"), self.kw("HOLD")));
            }
            CursorHoldability::WithoutHold => {
                s.push_str(&format!(" {} {}", self.kw("WITHOUT"), self.kw("HOLD")));
            }
            CursorHoldability::Default => {}
        }
        match &stmt.returnability {
            CursorReturnability::WithReturn => {
                s.push_str(&format!(" {} {}", self.kw("WITH"), self.kw("RETURN")));
            }
            CursorReturnability::WithoutReturn => {
                s.push_str(&format!(" {} {}", self.kw("WITHOUT"), self.kw("RETURN")));
            }
            CursorReturnability::Default => {}
        }
        match &stmt.return_to {
            CursorReturnTo::ToCaller => {
                s.push_str(&format!(" {} {}", self.kw("TO"), self.kw("CALLER")));
            }
            CursorReturnTo::ToClient => {
                s.push_str(&format!(" {} {}", self.kw("TO"), self.kw("CLIENT")));
            }
            CursorReturnTo::Default => {}
        }
        s.push_str(&format!(" {} {}", self.kw("CURSOR FOR"), self.format_select(&stmt.query)));
        s
    }

    fn format_close_portal(&self, stmt: &ClosePortalStatement) -> String {
        match &stmt.target {
            CloseTarget::Name(name) => format!("{} {}", self.kw("CLOSE"), name),
            CloseTarget::All => format!("{} {}", self.kw("CLOSE"), self.kw("ALL")),
        }
    }

    fn format_fetch(&self, stmt: &FetchStatement) -> String {
        let dir = self.format_direction(&stmt.direction);
        format!("{} {} {} {}", self.kw("FETCH"), dir, self.kw("FROM"), stmt.cursor_name)
    }

    fn format_move(&self, stmt: &MoveStatement) -> String {
        let dir = self.format_direction(&stmt.direction);
        format!("{} {} {} {}", self.kw("MOVE"), dir, self.kw("FROM"), stmt.cursor_name)
    }

    fn format_direction(&self, direction: &FetchDirection) -> String {
        match direction {
            FetchDirection::Next => self.kw("NEXT").to_string(),
            FetchDirection::Prior => self.kw("PRIOR").to_string(),
            FetchDirection::First => self.kw("FIRST").to_string(),
            FetchDirection::Last => self.kw("LAST").to_string(),
            FetchDirection::Absolute(n) => format!("{} {}", self.kw("ABSOLUTE"), n),
            FetchDirection::Relative(n) => format!("{} {}", self.kw("RELATIVE"), n),
            FetchDirection::Forward => self.kw("FORWARD").to_string(),
            FetchDirection::ForwardCount(n) => format!("{} {}", self.kw("FORWARD"), n),
            FetchDirection::ForwardAll => format!("{} {}", self.kw("FORWARD"), self.kw("ALL")),
            FetchDirection::Backward => self.kw("BACKWARD").to_string(),
            FetchDirection::BackwardCount(n) => format!("{} {}", self.kw("BACKWARD"), n),
            FetchDirection::BackwardAll => format!("{} {}", self.kw("BACKWARD"), self.kw("ALL")),
            FetchDirection::Count(n) => n.to_string(),
            FetchDirection::All => self.kw("ALL").to_string(),
        }
    }

    fn format_cluster(&self, stmt: &ClusterStatement) -> String {
        let mut s = self.kw("CLUSTER").to_string();
        if stmt.verbose {
            s.push(' ');
            s.push_str(&self.kw("VERBOSE"));
        }
        if let Some(ref table) = stmt.table {
            s.push(' ');
            s.push_str(&self.format_object_name(table));
        }
        if let Some(ref partition) = stmt.partition {
            s.push_str(&format!(" {} ({})", self.kw("PARTITION"), partition));
        }
        if let Some(ref idx) = stmt.using_index {
            s.push_str(&format!(" {} {}", self.kw("USING"), idx));
        }
        s
    }

    fn format_reindex(&self, stmt: &ReindexStatement) -> String {
        let target = match &stmt.target {
            ReindexTarget::Table(name) => {
                format!("{} {}", self.kw("TABLE"), self.format_object_name(name))
            }
            ReindexTarget::Index(name) => {
                format!("{} {}", self.kw("INDEX"), self.format_object_name(name))
            }
            ReindexTarget::Schema(name) => format!("{} {}", self.kw("SCHEMA"), name),
            ReindexTarget::Database(name) => format!("{} {}", self.kw("DATABASE"), name),
            ReindexTarget::System => self.kw("SYSTEM").to_string(),
        };
        let mut s = format!("{} {}", self.kw("REINDEX"), target);
        if stmt.concurrent {
            s.push(' ');
            s.push_str(&self.kw("CONCURRENTLY"));
        }
        if stmt.verbose {
            s.push(' ');
            s.push_str(&self.kw("VERBOSE"));
        }
        s
    }

    fn format_listen(&self, stmt: &ListenStatement) -> String {
        format!("{} {}", self.kw("LISTEN"), stmt.channel)
    }

    fn format_notify(&self, stmt: &NotifyStatement) -> String {
        let mut s = format!("{} {}", self.kw("NOTIFY"), stmt.channel);
        if let Some(ref payload) = stmt.payload {
            s.push_str(&format!(", '{}'", payload));
        }
        s
    }

    fn format_unlisten(&self, stmt: &UnlistenStatement) -> String {
        match &stmt.channel {
            Some(ch) => format!("{} {}", self.kw("UNLISTEN"), ch),
            None => self.kw("UNLISTEN").to_string(),
        }
    }

    fn format_rule(&self, stmt: &RuleStatement) -> String {
        let mut s = format!(
            "{} {} {} {} {} {}",
            self.kw("CREATE RULE"),
            stmt.name,
            self.kw("AS ON"),
            stmt.event,
            self.kw("TO"),
            self.format_object_name(&stmt.table)
        );
        if let Some(ref cond) = stmt.condition {
            s.push_str(&format!(" {} {}", self.kw("WHERE"), self.format_expr(cond)));
        }
        if stmt.instead {
            s.push_str(&self.kw(" DO INSTEAD"));
        } else {
            s.push_str(&self.kw(" DO"));
        }
        if stmt.actions.is_empty() || (stmt.actions.len() == 1 && stmt.actions[0] == "NOTHING") {
            s.push(' ');
            s.push_str(&self.kw("NOTHING"));
        }
        s
    }

    fn format_create_trigger(&self, stmt: &CreateTriggerStatement) -> String {
        let timing = match stmt.timing {
            TriggerTiming::Before => "BEFORE",
            TriggerTiming::After => "AFTER",
            TriggerTiming::InsteadOf => "INSTEAD OF",
        };
        let events: Vec<String> = stmt
            .events
            .iter()
            .map(|e| match e {
                TriggerEvent::Insert => "INSERT".to_string(),
                TriggerEvent::Update => "UPDATE".to_string(),
                TriggerEvent::UpdateOf(cols) => format!("UPDATE OF {}", cols.join(", ")),
                TriggerEvent::Delete => "DELETE".to_string(),
                TriggerEvent::Truncate => "TRUNCATE".to_string(),
            })
            .collect();
        let mut s = format!(
            "{} {} {} {} {} {} {}",
            self.kw("CREATE TRIGGER"),
            stmt.name,
            self.kw(timing),
            events.join(" OR "),
            self.kw("ON"),
            self.format_object_name(&stmt.table),
            self.kw("FOR EACH"),
        );
        s.push_str(match stmt.for_each {
            TriggerForEach::Row => " ROW",
            TriggerForEach::Statement => " STATEMENT",
        });
        if let Some(ref w) = stmt.when {
            s.push_str(&format!(" {} ({})", self.kw("WHEN"), self.format_expr(w)));
        }
        let execute_kw = match stmt.execute_kind {
            ExecuteKind::Function => "EXECUTE FUNCTION",
            ExecuteKind::Procedure => "EXECUTE PROCEDURE",
        };
        s.push_str(&format!(
            " {} {} {}",
            self.kw(execute_kw),
            self.format_object_name(&stmt.func_name),
            if stmt.func_args.is_empty() {
                String::new()
            } else {
                let args: Vec<String> = stmt.func_args.iter().map(|a| self.format_expr(a)).collect();
                format!("({})", args.join(", "))
            }
        ));
        s
    }

    fn format_create_materialized_view(&self, stmt: &CreateMaterializedViewStatement) -> String {
        let mut s = format!("{} {}", self.kw("CREATE MATERIALIZED VIEW"), self.format_object_name(&stmt.name));
        if !stmt.columns.is_empty() {
            s.push_str(&format!("({})", stmt.columns.join(", ")));
        }
        s.push_str(&format!(" {} {}", self.kw("AS"), self.format_select(&stmt.query)));
        if let Some(ref ts) = stmt.tablespace {
            s.push_str(&format!(" {} {}", self.kw("TABLESPACE"), ts));
        }
        if stmt.with_data {
            s.push_str(&format!(" {} {}", self.kw("WITH"), self.kw("DATA")));
        } else {
            s.push_str(&format!(" {} {} {}", self.kw("WITH"), self.kw("NO"), self.kw("DATA")));
        }
        s
    }

    fn format_refresh_matview(&self, stmt: &RefreshMatViewStatement) -> String {
        let mut s = format!("{} {}", self.kw("REFRESH MATERIALIZED VIEW"), self.format_object_name(&stmt.name));
        if stmt.concurrent {
            s.push(' ');
            s.push_str(&self.kw("CONCURRENTLY"));
        }
        s
    }

    fn format_alter_database(&self, stmt: &AlterDatabaseStatement) -> String {
        let action = match &stmt.action {
            AlterDatabaseAction::Set { parameter, value } => {
                format!("{} {} {} {}", self.kw("SET"), parameter, self.kw("TO"), value)
            }
            AlterDatabaseAction::Reset { parameter } => {
                format!("{} {}", self.kw("RESET"), parameter)
            }
            AlterDatabaseAction::RenameTo { new_name } => {
                format!("{} {}", self.kw("RENAME TO"), new_name)
            }
            AlterDatabaseAction::OwnerTo { owner } => {
                format!("{} {}", self.kw("OWNER TO"), owner)
            }
            AlterDatabaseAction::WithConnectionLimit { limit } => {
                format!("{} {}", self.kw("WITH CONNECTION LIMIT"), limit)
            }
            AlterDatabaseAction::EnablePrivateObject => self.kw("ENABLE PRIVATE OBJECT").to_string(),
        };
        format!("{} {} {}", self.kw("ALTER DATABASE"), stmt.name, action)
    }

    fn format_alter_schema(&self, stmt: &AlterSchemaStatement) -> String {
        let action = match &stmt.action {
            AlterSchemaAction::RenameTo { new_name } => {
                format!("{} {}", self.kw("RENAME TO"), new_name)
            }
            AlterSchemaAction::OwnerTo { owner } => format!("{} {}", self.kw("OWNER TO"), owner),
            AlterSchemaAction::CharacterSet { charset, collate } => {
                let mut s = format!("{} {}", self.kw("CHARACTER SET"), charset);
                if let Some(c) = collate {
                    s.push_str(&format!(" {} {}", self.kw("COLLATE"), c));
                }
                s
            }
        };
        format!("{} {} {}", self.kw("ALTER SCHEMA"), stmt.name, action)
    }

    fn format_alter_sequence(&self, stmt: &AlterSequenceStatement) -> String {
        let opts: Vec<String> = stmt
            .options
            .iter()
            .map(|o| match o {
                SequenceOption::IncrementBy(n) => format!("{} {}", self.kw("INCREMENT BY"), n),
                SequenceOption::MinValue(Some(n)) => format!("{} {}", self.kw("MINVALUE"), n),
                SequenceOption::MinValue(None) => self.kw("NO MINVALUE").to_string(),
                SequenceOption::MaxValue(Some(n)) => format!("{} {}", self.kw("MAXVALUE"), n),
                SequenceOption::MaxValue(None) => self.kw("NO MAXVALUE").to_string(),
                SequenceOption::StartWith(n) => format!("{} {}", self.kw("START WITH"), n),
                SequenceOption::Restart(_) => self.kw("RESTART").to_string(),
                SequenceOption::Cache(n) => format!("{} {}", self.kw("CACHE"), n),
                SequenceOption::Cycle(true) => self.kw("CYCLE").to_string(),
                SequenceOption::Cycle(false) | SequenceOption::NoCycle => self.kw("NO CYCLE").to_string(),
                SequenceOption::OwnedBy { owner } => {
                    format!("{} {}", self.kw("OWNED BY"), self.format_object_name(owner))
                }
                SequenceOption::OwnerTo { owner } => {
                    format!("{} {}", self.kw("OWNER TO"), self.format_object_name(owner))
                }
            })
            .collect();
        let prefix = if stmt.is_large { self.kw("ALTER LARGE SEQUENCE") } else { self.kw("ALTER SEQUENCE") };
        format!("{} {} {}", prefix, self.format_object_name(&stmt.name), opts.join(" "))
    }

    fn format_alter_function(&self, stmt: &AlterFunctionStatement) -> String {
        format!(
            "{} {} {}",
            self.kw("ALTER FUNCTION"),
            self.format_object_name(&stmt.name),
            self.format_alter_function_action(&stmt.action)
        )
    }

    fn format_alter_function_action(&self, action: &AlterFunctionAction) -> String {
        match action {
            AlterFunctionAction::RenameTo { new_name } => {
                format!("{} {}", self.kw("RENAME TO"), new_name)
            }
            AlterFunctionAction::OwnerTo { owner } => format!("{} {}", self.kw("OWNER TO"), owner),
            AlterFunctionAction::SetSchema { schema } => {
                format!("{} {}", self.kw("SET SCHEMA"), schema)
            }
            AlterFunctionAction::Set { parameter, value } => {
                format!("{} {} {} {}", self.kw("SET"), parameter, self.kw("TO"), value)
            }
            AlterFunctionAction::Reset { parameter } => {
                format!("{} {}", self.kw("RESET"), parameter)
            }
            AlterFunctionAction::Compile => "COMPILE".to_string(),
            AlterFunctionAction::Immutable => self.kw("IMMUTABLE").to_string(),
            AlterFunctionAction::Stable => self.kw("STABLE").to_string(),
            AlterFunctionAction::Volatile => self.kw("VOLATILE").to_string(),
            AlterFunctionAction::Leakproof { not: false } => self.kw("LEAKPROOF").to_string(),
            AlterFunctionAction::Leakproof { not: true } => {
                format!("{} {}", self.kw("NOT"), self.kw("LEAKPROOF"))
            }
            AlterFunctionAction::Strict => self.kw("STRICT").to_string(),
            AlterFunctionAction::CalledOnNullInput => self.kw("CALLED ON NULL INPUT").to_string(),
            AlterFunctionAction::ReturnsNullOnNullInput => self.kw("RETURNS NULL ON NULL INPUT").to_string(),
            AlterFunctionAction::Shippable { not: false } => self.kw("SHIPPABLE").to_string(),
            AlterFunctionAction::Shippable { not: true } => {
                format!("{} {}", self.kw("NOT"), self.kw("SHIPPABLE"))
            }
            AlterFunctionAction::Package { not: false } => self.kw("PACKAGE").to_string(),
            AlterFunctionAction::Package { not: true } => {
                format!("{} {}", self.kw("NOT"), self.kw("PACKAGE"))
            }
        }
    }

    fn format_alter_role(&self, stmt: &AlterRoleStatement) -> String {
        let opts: Vec<String> = stmt
            .options
            .iter()
            .map(|(k, v)| match v {
                Some(val) => format!("{} {}", k, val),
                None => k.clone(),
            })
            .collect();
        format!("{} {} {}", self.kw("ALTER ROLE"), stmt.name, opts.join(" "))
    }

    fn format_alter_user(&self, stmt: &AlterUserStatement) -> String {
        let opts: Vec<String> = stmt
            .options
            .iter()
            .map(|(k, v)| match v {
                Some(val) => format!("{} {}", k, val),
                None => k.clone(),
            })
            .collect();
        format!("{} {} {}", self.kw("ALTER USER"), stmt.name, opts.join(" "))
    }

    fn format_alter_global_config(&self, stmt: &AlterGlobalConfigStatement) -> String {
        match &stmt.action {
            AlterGlobalConfigAction::Set { parameter, value } => {
                format!("{} {} {} {}", self.kw("ALTER SYSTEM SET"), parameter, self.kw("="), value)
            }
            AlterGlobalConfigAction::Reset { parameter } => {
                format!("{} {}", self.kw("ALTER SYSTEM RESET"), parameter)
            }
        }
    }

    // ── Additional formatters ──

    fn format_routine_param(&self, param: &RoutineParam) -> String {
        let mut s = param.name.clone();
        if let Some(ref mode) = param.mode {
            s.push(' ');
            s.push_str(mode);
        }
        s.push(' ');
        s.push_str(&param.data_type);
        if let Some(ref default_val) = param.default_value {
            s.push_str(&format!(" {} {}", self.kw("DEFAULT"), default_val));
        }
        s
    }

    fn format_create_function(&self, stmt: &CreateFunctionStatement) -> String {
        let mut s = self.kw("CREATE").to_string();
        if stmt.replace {
            s.push_str(&format!(" {} {}", self.kw("OR"), self.kw("REPLACE")));
        }
        s.push_str(&format!("{} {}", self.kw(" FUNCTION"), self.format_object_name(&stmt.name)));
        let params: Vec<String> = stmt.parameters.iter().map(|p| self.format_routine_param(p)).collect();
        s.push_str(&format!("({})", params.join(", ")));
        if let Some(rt) = &stmt.return_type {
            s.push_str(&format!(" {} {}", self.kw("RETURNS"), rt));
        }
        if let Some(block) = &stmt.block {
            s.push_str(&format!(" {} {}", self.kw("AS"), self.format_pl_block_named(block, 0)));
        }
        let opts_str = self.format_function_options(&stmt.options);
        if !opts_str.is_empty() {
            s.push_str(&format!(" {}", opts_str));
        }
        s
    }

    fn format_create_procedure(&self, stmt: &CreateProcedureStatement) -> String {
        let mut s = self.kw("CREATE").to_string();
        if stmt.replace {
            s.push_str(&format!(" {} {}", self.kw("OR"), self.kw("REPLACE")));
        }
        s.push_str(&format!("{} {}", self.kw(" PROCEDURE"), self.format_object_name(&stmt.name)));
        let params: Vec<String> = stmt.parameters.iter().map(|p| self.format_routine_param(p)).collect();
        s.push_str(&format!("({})", params.join(", ")));
        if let Some(block) = &stmt.block {
            s.push_str(&format!(" {} {}", self.kw("AS"), self.format_pl_block_named(block, 0)));
        }
        let opts_str = self.format_function_options(&stmt.options);
        if !opts_str.is_empty() {
            s.push_str(&format!(" {}", opts_str));
        }
        s
    }

    fn format_function_options(&self, opts: &FunctionOptions) -> String {
        let mut parts = Vec::new();
        if let Some(lang) = &opts.language {
            parts.push(format!("{} {}", self.kw("LANGUAGE"), lang));
        }
        match &opts.volatility {
            Some(Volatility::Immutable) => parts.push(self.kw("IMMUTABLE").to_string()),
            Some(Volatility::Stable) => parts.push(self.kw("STABLE").to_string()),
            Some(Volatility::Volatile) => parts.push(self.kw("VOLATILE").to_string()),
            None => {}
        }
        if let Some(strict) = opts.strict {
            if strict {
                parts.push(self.kw("STRICT").to_string());
            }
        }
        if let Some(cost) = opts.cost {
            parts.push(format!("{} {}", self.kw("COST"), cost));
        }
        if let Some(rows) = opts.rows {
            parts.push(format!("{} {}", self.kw("ROWS"), rows));
        }
        if let Some(lp) = opts.leakproof {
            if lp {
                parts.push(self.kw("LEAKPROOF").to_string());
            } else {
                parts.push(format!("{} {}", self.kw("NOT"), self.kw("LEAKPROOF")));
            }
        }
        match &opts.security {
            Some(SecurityMode::Invoker) => parts.push(self.kw("SECURITY INVOKER").to_string()),
            Some(SecurityMode::Definer) => parts.push(self.kw("SECURITY DEFINER").to_string()),
            None => {}
        }
        match &opts.parallel {
            Some(ParallelMode::Safe) => parts.push(self.kw("PARALLEL SAFE").to_string()),
            Some(ParallelMode::Unsafe) => parts.push(self.kw("PARALLEL UNSAFE").to_string()),
            Some(ParallelMode::Restricted) => parts.push(self.kw("PARALLEL RESTRICTED").to_string()),
            None => {}
        }
        if let Some(fenced) = opts.fenced {
            if fenced {
                parts.push(self.kw("FENCED").to_string());
            } else {
                parts.push(format!("{} {}", self.kw("NOT"), self.kw("FENCED")));
            }
        }
        if let Some(shippable) = opts.shippable {
            if shippable {
                parts.push(self.kw("SHIPPABLE").to_string());
            } else {
                parts.push(format!("{} {}", self.kw("NOT"), self.kw("SHIPPABLE")));
            }
        }
        if !opts.extra.is_empty() {
            parts.push(opts.extra.clone());
        }
        parts.join(" ")
    }

    fn format_create_package(&self, stmt: &CreatePackageStatement) -> String {
        let mut s = self.kw("CREATE").to_string();
        if stmt.replace {
            s.push_str(&format!(" {}", self.kw("OR REPLACE")));
        }
        s.push_str(&format!(" {} {}", self.kw("PACKAGE"), self.format_object_name(&stmt.name)));
        if let Some(authid) = &stmt.authid {
            match authid {
                PackageAuthid::CurrentUser => s.push_str(&format!(" {}", self.kw("AUTHID CURRENT_USER"))),
                PackageAuthid::Definer => s.push_str(&format!(" {}", self.kw("AUTHID DEFINER"))),
            }
        }
        s.push_str(&format!(" {}", self.kw("AS")));
        for item in &stmt.items {
            s.push('\n');
            s.push_str(&Self::pad(1));
            s.push_str(&self.format_package_item(item, 1));
        }
        s.push('\n');
        s.push_str(&self.kw("END").to_string());
        s.push(';');
        s
    }

    fn format_create_package_body(&self, stmt: &CreatePackageBodyStatement) -> String {
        let mut s = self.kw("CREATE").to_string();
        if stmt.replace {
            s.push_str(&format!(" {}", self.kw("OR REPLACE")));
        }
        s.push_str(&format!(" {} {}", self.kw("PACKAGE BODY"), self.format_object_name(&stmt.name)));
        s.push_str(&format!(" {}", self.kw("AS")));
        for item in &stmt.items {
            s.push('\n');
            s.push_str(&Self::pad(1));
            s.push_str(&self.format_package_item(item, 1));
        }
        s.push('\n');
        s.push_str(&self.kw("END").to_string());
        s.push(';');
        s
    }

    fn format_package_item(&self, item: &PackageItem, indent: usize) -> String {
        use crate::ast::plpgsql::*;
        match item {
            PackageItem::Procedure(p) => self.format_package_procedure(p, indent),
            PackageItem::Function(f) => self.format_package_function(f, indent),
            PackageItem::Variable(v) => {
                let mut s = format!("{} {}", v.name, self.format_pl_data_type(&v.data_type));
                if let Some(ref expr) = v.default {
                    s.push_str(&format!(" := {}", self.format_expr(expr)));
                }
                s.push(';');
                s
            }
            PackageItem::Type(t) => {
                let inner = match t {
                    PlTypeDecl::Record { name, fields } => {
                        let fields_str = fields
                            .iter()
                            .map(|f| format!("{} {}", f.name, self.format_pl_data_type(&f.data_type)))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{} {} {} ({})", self.kw("TYPE"), name, self.kw("IS RECORD"), fields_str)
                    }
                    PlTypeDecl::TableOf { name, elem_type, index_by } => {
                        let mut s = format!(
                            "{} {} {} {} {}",
                            self.kw("TYPE"),
                            name,
                            self.kw("IS TABLE OF"),
                            self.format_pl_data_type(elem_type),
                            ""
                        );
                        if let Some(idx) = index_by {
                            s = format!("{} {} {}", s.trim(), self.kw("INDEX BY"), self.format_pl_data_type(idx));
                        }
                        s.trim().to_string()
                    }
                    PlTypeDecl::VarrayOf { name, size, elem_type } => {
                        format!(
                            "{} {} {} ({}) {} {}",
                            self.kw("TYPE"),
                            name,
                            self.kw("IS VARRAY"),
                            self.format_expr(size),
                            self.kw("OF"),
                            self.format_pl_data_type(elem_type)
                        )
                    }
                    PlTypeDecl::RefCursor { name } => {
                        format!("{} {} {}", self.kw("TYPE"), name, self.kw("IS REF CURSOR"))
                    }
                };
                format!("{};", inner)
            }
            PackageItem::Cursor(c) => {
                let mut s = format!("{} {}", self.kw("CURSOR"), c.name);
                if !c.arguments.is_empty() {
                    let args: Vec<String> = c
                        .arguments
                        .iter()
                        .map(|a| {
                            let mode = match a.mode {
                                PlArgMode::In => self.kw("IN"),
                                PlArgMode::Out => self.kw("OUT"),
                                PlArgMode::InOut => format!("{} {}", self.kw("IN"), self.kw("OUT")),
                            };
                            format!("{} {} {}", a.name, mode, self.format_pl_data_type(&a.data_type))
                        })
                        .collect();
                    s.push_str(&format!(" ({})", args.join(", ")));
                }
                if !c.query.is_empty() {
                    s.push_str(&format!(" {} {}", self.kw("IS"), c.query));
                }
                s.push(';');
                s
            }
            PackageItem::Raw(s) => s.clone(),
        }
    }

    fn format_package_procedure(&self, proc: &PackageProcedure, indent: usize) -> String {
        let mut s = format!("{} {}", self.kw("PROCEDURE"), self.format_object_name(&proc.name));
        if !proc.parameters.is_empty() {
            let params: Vec<String> = proc.parameters.iter().map(|p| self.format_routine_param(p)).collect();
            s.push_str(&format!("({})", params.join(", ")));
        }
        if let Some(ref block) = proc.block {
            s.push_str(&format!(" {} {}", self.kw("IS"), self.format_pl_block_named(block, indent)));
            s.push(';');
        } else {
            s.push(';');
        }
        s
    }

    fn format_package_function(&self, func: &PackageFunction, indent: usize) -> String {
        let mut s = format!("{} {}", self.kw("FUNCTION"), self.format_object_name(&func.name));
        if !func.parameters.is_empty() {
            let params: Vec<String> = func.parameters.iter().map(|p| self.format_routine_param(p)).collect();
            s.push_str(&format!("({})", params.join(", ")));
        }
        if let Some(ref rt) = func.return_type {
            s.push_str(&format!(" {} {}", self.kw("RETURN"), rt));
        }
        if let Some(ref block) = func.block {
            s.push_str(&format!(" {} {}", self.kw("IS"), self.format_pl_block_named(block, indent)));
            s.push(';');
        } else {
            s.push(';');
        }
        s
    }

    fn format_create_extension(&self, stmt: &CreateExtensionStatement) -> String {
        let mut s = self.kw("CREATE").to_string();
        if stmt.replace {
            s.push_str(&format!(" {}", self.kw("OR REPLACE")));
        }
        s.push_str(&format!(" {}", self.kw("EXTENSION")));
        if stmt.if_not_exists {
            s.push_str(&format!(" {}", self.kw("IF NOT EXISTS")));
        }
        s.push_str(&format!(" {}", stmt.name));
        if let Some(schema) = &stmt.schema {
            s.push_str(&format!(" {} {}", self.kw("SCHEMA"), schema));
        }
        if let Some(version) = &stmt.version {
            s.push_str(&format!(" {} {}", self.kw("VERSION"), version));
        }
        if stmt.cascade {
            s.push_str(&format!(" {}", self.kw("CASCADE")));
        }
        s
    }

    fn format_create_role(&self, stmt: &CreateRoleStatement) -> String {
        format!("{} {}", self.kw("CREATE ROLE"), stmt.name)
    }

    fn format_create_user(&self, stmt: &CreateUserStatement) -> String {
        let mut s = format!("{} {}", self.kw("CREATE USER"), self.quote_identifier(&stmt.name));
        for opt in &stmt.options {
            s.push(' ');
            s.push_str(&self.format_role_option(opt));
        }
        s
    }

    fn format_role_option(&self, opt: &RoleOption) -> String {
        match opt {
            RoleOption::Superuser(true) => self.kw("SUPERUSER"),
            RoleOption::Superuser(false) => self.kw("NOSUPERUSER"),
            RoleOption::CreateDb(true) => self.kw("CREATEDB"),
            RoleOption::CreateDb(false) => self.kw("NOCREATEDB"),
            RoleOption::CreateRole(true) => self.kw("CREATEROLE"),
            RoleOption::CreateRole(false) => self.kw("NOCREATEROLE"),
            RoleOption::Inherit(true) => self.kw("INHERIT"),
            RoleOption::Inherit(false) => self.kw("NOINHERIT"),
            RoleOption::Login(true) => self.kw("LOGIN"),
            RoleOption::Login(false) => self.kw("NOLOGIN"),
            RoleOption::Replication(true) => self.kw("REPLICATION"),
            RoleOption::Replication(false) => self.kw("NOREPLICATION"),
            RoleOption::BypassRls(true) => self.kw("BYPASSRLS"),
            RoleOption::BypassRls(false) => self.kw("NOBYPASSRLS"),
            RoleOption::ConnectionLimit(n) => format!("{} {}", self.kw("CONNECTION LIMIT"), n),
            RoleOption::EncryptedPassword(p) => {
                format!("{} {}", self.kw("ENCRYPTED PASSWORD"), self.format_literal(&Literal::String(p.clone())))
            }
            RoleOption::UnencryptedPassword(p) => {
                format!("{} {}", self.kw("PASSWORD"), self.format_literal(&Literal::String(p.clone())))
            }
            RoleOption::ValidUntil(t) => {
                format!("{} {}", self.kw("VALID UNTIL"), self.format_literal(&Literal::String(t.clone())))
            }
            RoleOption::InRole(roles) => format!("{} {}", self.kw("IN ROLE"), roles.join(", ")),
            RoleOption::Role(roles) => format!("{} {}", self.kw("ROLE"), roles.join(", ")),
            RoleOption::Admin(roles) => format!("{} {}", self.kw("ADMIN"), roles.join(", ")),
            RoleOption::User(users) => format!("{} {}", self.kw("USER"), users.join(", ")),
            RoleOption::Sysid(id) => format!("{} {}", self.kw("SYSID"), id),
            RoleOption::DefaultTablespace(ts) => {
                format!("{} {}", self.kw("DEFAULT TABLESPACE"), self.quote_identifier(ts))
            }
            RoleOption::Tablespace(ts) => {
                format!("{} {}", self.kw("TABLESPACE"), self.quote_identifier(ts))
            }
            RoleOption::Profile(p) => {
                format!("{} {}", self.kw("PROFILE"), self.quote_identifier(p))
            }
            RoleOption::AccountLock(true) => self.kw("ACCOUNT LOCK"),
            RoleOption::AccountLock(false) => self.kw("ACCOUNT UNLOCK"),
            RoleOption::AuditAdmin(true) => self.kw("AUDITADMIN"),
            RoleOption::AuditAdmin(false) => self.kw("NOAUDITADMIN"),
            RoleOption::MonAdmin(true) => self.kw("MONADMIN"),
            RoleOption::MonAdmin(false) => self.kw("NOMONADMIN"),
            RoleOption::OprAdmin(true) => self.kw("OPRADMIN"),
            RoleOption::OprAdmin(false) => self.kw("NOOPRADMIN"),
            RoleOption::PolAdmin(true) => self.kw("POLADMIN"),
            RoleOption::PolAdmin(false) => self.kw("NOPOLADMIN"),
            RoleOption::Persistence(true) => self.kw("PERSISTENCE"),
            RoleOption::Persistence(false) => self.kw("NOPERSISTENCE"),
            RoleOption::Independent(true) => self.kw("INDEPENDENT"),
            RoleOption::Independent(false) => self.kw("NOINDEPENDENT"),
            RoleOption::Useft(true) => self.kw("USEFT"),
            RoleOption::Useft(false) => self.kw("NOUSEFT"),
            RoleOption::VcAdmin(true) => self.kw("VCADMIN"),
            RoleOption::VcAdmin(false) => self.kw("NOVCADMIN"),
            RoleOption::Permit(true) => self.kw("PERMIT"),
            RoleOption::Permit(false) => self.kw("NOPERMIT"),
        }
    }

    fn format_create_group(&self, stmt: &CreateGroupStatement) -> String {
        format!("{} {}", self.kw("CREATE GROUP"), stmt.name)
    }

    fn format_grant_role(&self, stmt: &GrantRoleStatement) -> String {
        let mut s = format!("{} {}", self.kw("GRANT"), stmt.roles.join(", "));
        s.push_str(&format!(" {} {}", self.kw("TO"), stmt.grantees.join(", ")));
        if stmt.with_admin_option {
            s.push_str(&format!(" {}", self.kw("WITH ADMIN OPTION")));
        }
        if let Some(by) = &stmt.granted_by {
            s.push_str(&format!(" {} {}", self.kw("GRANTED BY"), by));
        }
        s
    }

    fn format_revoke_role(&self, stmt: &RevokeRoleStatement) -> String {
        let mut s = format!("{} {}", self.kw("REVOKE"), stmt.roles.join(", "));
        s.push_str(&format!(" {} {}", self.kw("FROM"), stmt.grantees.join(", ")));
        if stmt.cascade {
            s.push_str(&format!(" {}", self.kw("CASCADE")));
        }
        if let Some(by) = &stmt.granted_by {
            s.push_str(&format!(" {} {}", self.kw("GRANTED BY"), by));
        }
        s
    }

    fn format_alter_group(&self, stmt: &AlterGroupStatement) -> String {
        let action = match &stmt.action {
            AlterGroupAction::AddUser(user) => format!("{} {}", self.kw("ADD USER"), user),
            AlterGroupAction::DropUser(user) => format!("{} {}", self.kw("DROP USER"), user),
            AlterGroupAction::RenameTo(new_name) => {
                format!("{} {}", self.kw("RENAME TO"), new_name)
            }
        };
        format!("{} {} {}", self.kw("ALTER GROUP"), stmt.name, action)
    }

    fn format_alter_composite_type(&self, stmt: &AlterCompositeTypeStatement) -> String {
        let action = match &stmt.action {
            AlterTypeAction::AddAttribute { name, data_type, cascade } => {
                let mut s = format!("{} {} {} {}", self.kw("ADD ATTRIBUTE"), name, self.kw("TYPE"), data_type);
                if *cascade {
                    s.push_str(&format!(" {}", self.kw("CASCADE")));
                }
                s
            }
            AlterTypeAction::DropAttribute { name, if_exists, cascade } => {
                let mut s = format!("{} {}", self.kw("DROP ATTRIBUTE"), name);
                if *if_exists {
                    s.push_str(&format!(" {}", self.kw("IF EXISTS")));
                }
                if *cascade {
                    s.push_str(&format!(" {}", self.kw("CASCADE")));
                }
                s
            }
            AlterTypeAction::RenameAttribute { old_name, new_name, cascade } => {
                let mut s =
                    format!("{} {} {} {} {}", self.kw("RENAME ATTRIBUTE"), old_name, self.kw("TO"), new_name, "");
                if *cascade {
                    s.push_str(&format!(" {}", self.kw("CASCADE")));
                }
                s.trim_end().to_string()
            }
            AlterTypeAction::RenameTo(new_name) => format!("{} {}", self.kw("RENAME TO"), new_name),
            AlterTypeAction::SetSchema(schema) => format!("{} {}", self.kw("SET SCHEMA"), schema),
            AlterTypeAction::OwnerTo(owner) => format!("{} {}", self.kw("OWNER TO"), owner),
            AlterTypeAction::AddEnumValue { if_not_exists, value, before, after } => {
                let ine = if *if_not_exists { self.kw("IF NOT EXISTS") } else { String::new() };
                let mut s = format!("{} {} {}", self.kw("ADD VALUE"), ine, value);
                if let Some(b) = before {
                    s = format!("{} {} {}", s, self.kw("BEFORE"), b);
                }
                if let Some(a) = after {
                    s = format!("{} {} {}", s, self.kw("AFTER"), a);
                }
                s.trim().to_string()
            }
            AlterTypeAction::RenameEnumValue { old_value, new_value } => {
                format!("{} {} {} {}", self.kw("RENAME VALUE"), old_value, self.kw("TO"), new_value)
            }
        };
        format!("{} {} {}", self.kw("ALTER TYPE"), self.format_object_name(&stmt.name), action)
    }

    fn format_alter_view(&self, stmt: &AlterViewStatement) -> String {
        let action = match &stmt.action {
            AlterViewAction::RenameTo(new_name) => format!("{} {}", self.kw("RENAME TO"), new_name),
            AlterViewAction::Set(opts) => {
                format!("{} {}", self.kw("SET"), self.format_options(opts))
            }
            AlterViewAction::Reset(params) => {
                format!("{} {}", self.kw("RESET"), params.join(", "))
            }
            AlterViewAction::SetSchema(schema) => format!("{} {}", self.kw("SET SCHEMA"), schema),
            AlterViewAction::OwnerTo(owner) => format!("{} {}", self.kw("OWNER TO"), owner),
            AlterViewAction::AlterColumnDefault { column, set_default } => match set_default {
                Some(val) => {
                    format!("{} {} {} {} {}", self.kw("ALTER COLUMN"), column, self.kw("SET DEFAULT"), "", val)
                }
                None => format!("{} {} {}", self.kw("ALTER COLUMN"), column, self.kw("DROP DEFAULT")),
            },
        };
        format!("{} {} {}", self.kw("ALTER VIEW"), self.format_object_name(&stmt.name), action)
    }

    fn format_alter_trigger(&self, stmt: &AlterTriggerStatement) -> String {
        if let Some(enable) = stmt.enable {
            let action = if enable { "ENABLE" } else { "DISABLE" };
            format!("{} {} {}", self.kw("ALTER TRIGGER"), stmt.name, self.kw(action))
        } else {
            format!(
                "{} {} {} {} {} {}",
                self.kw("ALTER TRIGGER"),
                stmt.name,
                self.kw("ON"),
                self.format_object_name(stmt.table.as_ref().expect("ALTER TRIGGER must have table")),
                self.kw("RENAME TO"),
                stmt.new_name.as_ref().expect("ALTER TRIGGER RENAME must have new_name")
            )
        }
    }

    fn format_alter_extension(&self, stmt: &AlterExtensionStatement) -> String {
        format!("{} {} {}", self.kw("ALTER EXTENSION"), stmt.name, stmt.action)
    }

    fn format_create_cast(&self, stmt: &CreateCastStatement) -> String {
        let mut s = format!(
            "{} ({}) {} ({})",
            self.kw("CREATE CAST"),
            self.format_data_type(&stmt.source_type),
            self.kw("AS"),
            self.format_data_type(&stmt.target_type)
        );
        match &stmt.method {
            CastMethod::WithFunction(func) => s.push_str(&format!(" {} {}", self.kw("WITH FUNCTION"), func)),
            CastMethod::WithoutFunction => s.push_str(&format!(" {}", self.kw("WITHOUT FUNCTION"))),
            CastMethod::WithInout => s.push_str(&format!(" {}", self.kw("WITH INOUT"))),
        }
        if let Some(ctx) = &stmt.context {
            match ctx {
                CastContext::Implicit => s.push_str(&format!(" {}", self.kw("AS IMPLICIT"))),
                CastContext::Assignment => s.push_str(&format!(" {}", self.kw("AS ASSIGNMENT"))),
            }
        }
        s
    }

    fn format_create_domain(&self, stmt: &CreateDomainStatement) -> String {
        let mut s = format!(
            "{} {} {}",
            self.kw("CREATE DOMAIN"),
            self.format_object_name(&stmt.name),
            self.format_data_type(&stmt.data_type)
        );
        if let Some(def) = &stmt.default_value {
            s.push_str(&format!(" {} {}", self.kw("DEFAULT"), self.format_expr(def)));
        }
        if stmt.not_null {
            s.push_str(&format!(" {}", self.kw("NOT NULL")));
        }
        if let Some(check) = &stmt.check {
            s.push_str(&format!(" {} ({})", self.kw("CHECK"), self.format_expr(check)));
        }
        s
    }

    fn format_create_table_as(&self, stmt: &CreateTableAsStatement) -> String {
        let mut s = self.kw("CREATE").to_string();
        if stmt.temporary {
            s.push(' ');
            s.push_str(&self.kw("TEMPORARY"));
        }
        if stmt.unlogged {
            s.push(' ');
            s.push_str(&self.kw("UNLOGGED"));
        }
        s.push(' ');
        s.push_str(&self.kw("TABLE"));
        if stmt.if_not_exists {
            s.push(' ');
            s.push_str(&self.kw("IF NOT EXISTS"));
        }
        s.push(' ');
        s.push_str(&self.format_object_name(&stmt.name));
        if !stmt.column_names.is_empty() {
            s.push_str(&format!(" ({})", stmt.column_names.join(", ")));
        }
        s.push(' ');
        s.push_str(&self.kw("AS"));
        if let Some(ref table) = stmt.as_table {
            s.push(' ');
            s.push_str(&self.kw("TABLE"));
            s.push(' ');
            s.push_str(&self.format_object_name(table));
        } else {
            s.push(' ');
            s.push_str(&self.format_select(&stmt.query));
        }
        if !stmt.with_data {
            s.push(' ');
            s.push_str(&self.kw("WITH NO DATA"));
        }
        s
    }

    fn format_create_aggregate(&self, stmt: &CreateAggregateStatement) -> String {
        let mut s = format!("{} {}", self.kw("CREATE"), self.kw("AGGREGATE"));
        s.push_str(&format!(" {}", stmt.name));
        if !stmt.base_types.is_empty() {
            s.push('(');
            s.push_str(&stmt.base_types.iter().map(|t| self.format_data_type(t)).collect::<Vec<_>>().join(", "));
            s.push(')');
        }
        if !stmt.options.is_empty() {
            let pairs: Vec<String> = stmt.options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
            s.push_str(&format!(" ({})", pairs.join(", ")));
        }
        s
    }

    fn format_create_operator(&self, stmt: &CreateOperatorStatement) -> String {
        let mut s = format!("{} {}", self.kw("CREATE"), self.kw("OPERATOR"));
        s.push_str(&format!(" {}", stmt.name));
        if !stmt.options.is_empty() {
            let pairs: Vec<String> = stmt.options.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
            s.push_str(&format!(" ({})", pairs.join(", ")));
        }
        s
    }

    fn format_alter_default_privileges(&self, stmt: &AlterDefaultPrivilegesStatement) -> String {
        let mut s = format!("{} {}", self.kw("ALTER"), self.kw("DEFAULT PRIVILEGES"));
        if let Some(role) = &stmt.role {
            s.push_str(&format!(" {} {} {}", self.kw("FOR"), self.kw("ROLE"), role));
        }
        if let Some(schema) = &stmt.schema {
            s.push_str(&format!(" {} {} {}", self.kw("IN"), self.kw("SCHEMA"), schema));
        }
        s.push(' ');
        match &stmt.action {
            DefaultPrivilegeAction::Grant(g) => s.push_str(&self.format_grant(g)),
            DefaultPrivilegeAction::Revoke(r) => s.push_str(&self.format_revoke(r)),
        }
        s
    }

    fn format_create_user_mapping(&self, stmt: &CreateUserMappingStatement) -> String {
        let mut s = format!("{} {}", self.kw("CREATE"), self.kw("USER MAPPING"));
        if stmt.if_not_exists {
            s.push_str(&format!(" {}", self.kw("IF NOT EXISTS")));
        }
        s.push_str(&format!(" {} {} {}", self.kw("FOR"), stmt.user_name, self.kw("SERVER")));
        s.push_str(&format!(" {}", self.format_object_name(&stmt.server)));
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_alter_user_mapping(&self, stmt: &AlterUserMappingStatement) -> String {
        let mut s = format!("{} {}", self.kw("ALTER"), self.kw("USER MAPPING"));
        s.push_str(&format!(" {} {} {}", self.kw("FOR"), stmt.user_name, self.kw("SERVER")));
        s.push_str(&format!(" {}", self.format_object_name(&stmt.server)));
        s.push_str(&self.format_options(&stmt.options));
        s
    }

    fn format_drop_user_mapping(&self, stmt: &DropUserMappingStatement) -> String {
        let mut s = format!("{} {}", self.kw("DROP"), self.kw("USER MAPPING"));
        if stmt.if_exists {
            s.push_str(&format!(" {}", self.kw("IF EXISTS")));
        }
        s.push_str(&format!(" {} {} {}", self.kw("FOR"), stmt.user_name, self.kw("SERVER")));
        s.push_str(&format!(" {}", self.format_object_name(&stmt.server)));
        s
    }

    fn format_shutdown(&self, stmt: &ShutdownStatement) -> String {
        let mut s = self.kw("SHUTDOWN");
        if let Some(mode) = &stmt.mode {
            s.push(' ');
            s.push_str(&self.kw(mode));
        }
        s
    }

    fn format_barrier(&self, stmt: &BarrierStatement) -> String {
        format!("{} {}", self.kw("BARRIER"), stmt.name)
    }

    fn format_purge(&self, stmt: &PurgeStatement) -> String {
        let mut s = self.kw("PURGE");
        match &stmt.target {
            PurgeTarget::Table { name } => {
                s.push(' ');
                s.push_str(&self.kw("TABLE"));
                s.push(' ');
                s.push_str(&self.format_object_name(name));
            }
            PurgeTarget::Index { name } => {
                s.push(' ');
                s.push_str(&self.kw("INDEX"));
                s.push(' ');
                s.push_str(&self.format_object_name(name));
            }
            PurgeTarget::RecycleBin => {
                s.push(' ');
                s.push_str(&self.kw("RECYCLEBIN"));
            }
            PurgeTarget::Snapshot { name } => {
                s.push(' ');
                s.push_str(&self.kw("SNAPSHOT"));
                s.push(' ');
                s.push_str(name);
            }
        }
        s
    }

    fn format_snapshot(&self, stmt: &SnapshotStatement) -> String {
        let mut s = self.kw("SNAPSHOT");
        if let Some(name) = &stmt.name {
            s.push(' ');
            s.push_str(name);
        }
        for (key, value) in &stmt.options {
            s.push(' ');
            s.push_str(key);
            if !value.is_empty() {
                s.push('=');
                s.push_str(value);
            }
        }
        s
    }

    fn format_timecapsule(&self, stmt: &TimeCapsuleStatement) -> String {
        format!(
            "{} {} {} {}",
            self.kw("TIMECAPSULE"),
            self.kw("TABLE"),
            self.format_object_name(&stmt.table_name),
            stmt.action
        )
    }

    fn format_shrink(&self, stmt: &ShrinkStatement) -> String {
        let mut s = self.kw("SHRINK");
        if let Some(target) = &stmt.target {
            s.push(' ');
            s.push_str(target);
        }
        if !stmt.raw_rest.is_empty() {
            s.push(' ');
            s.push_str(&stmt.raw_rest);
        }
        s
    }

    fn format_verify(&self, stmt: &VerifyStatement) -> String {
        let mut s = self.kw("VERIFY");
        if !stmt.raw_rest.is_empty() {
            s.push(' ');
            s.push_str(&stmt.raw_rest);
        }
        s
    }

    fn format_clean_conn(&self, stmt: &CleanConnStatement) -> String {
        let mut s = format!("{} {} {} {}", self.kw("CLEAN"), self.kw("CONNECTION"), self.kw("TO"), self.kw("ALL"));
        if stmt.force {
            s.push_str(&format!(" {}", self.kw("FORCE")));
        }
        if let Some(db) = &stmt.for_database {
            s.push_str(&format!(" {} {} {}", self.kw("FOR"), self.kw("DATABASE"), db));
        }
        if let Some(user) = &stmt.to_user {
            s.push_str(&format!(" {} {} {}", self.kw("TO"), self.kw("USER"), user));
        }
        s
    }

    fn format_compile(&self, stmt: &CompileStatement) -> String {
        let mut s = self.kw("COMPILE");
        if !stmt.raw_rest.is_empty() {
            s.push(' ');
            s.push_str(&stmt.raw_rest);
        }
        s
    }

    fn format_sec_label(&self, stmt: &SecLabelStatement) -> String {
        let mut s = format!("{} {} {}", self.kw("SECURITY"), self.kw("LABEL"), stmt.object_type);
        s.push_str(&format!(" {}", self.format_object_name(&stmt.name)));
        if let Some(provider) = &stmt.provider {
            s.push_str(&format!(" {} {}", self.kw("FOR"), provider));
        }
        if let Some(label) = &stmt.label {
            s.push_str(&format!(" {} '{}'", self.kw("IS"), label));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::token::tokenizer::Tokenizer;

    fn parse_one(sql: &str) -> Statement {
        let tokens = Tokenizer::new(sql).tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse();
        stmts.into_iter().next().unwrap()
    }

    #[test]
    fn test_format_select_simple() {
        let sql = "SELECT id, name FROM users";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("SELECT"));
        assert!(formatted.contains("FROM"));
    }

    #[test]
    fn test_format_select_with_where() {
        let sql = "SELECT * FROM users WHERE id = 1";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("WHERE"));
        assert!(formatted.contains("="));
    }

    #[test]
    fn test_format_insert_values() {
        let sql = "INSERT INTO users (id, name) VALUES (1, 'Alice')";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("INSERT INTO"));
        assert!(formatted.contains("VALUES"));
    }

    #[test]
    fn test_format_update() {
        let sql = "UPDATE users SET name = 'Bob' WHERE id = 1";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("UPDATE"));
        assert!(formatted.contains("SET"));
        assert!(formatted.contains("WHERE"));
    }

    #[test]
    fn test_format_delete() {
        let sql = "DELETE FROM users WHERE id = 1";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("DELETE FROM"));
        assert!(formatted.contains("WHERE"));
    }

    #[test]
    fn test_format_create_table() {
        let sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL)";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("CREATE TABLE"));
        assert!(formatted.contains("INTEGER"));
        assert!(formatted.contains("VARCHAR"));
    }

    #[test]
    fn test_format_create_index() {
        let sql = "CREATE INDEX idx_name ON users (name)";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("CREATE INDEX"));
        assert!(formatted.contains("ON"));
    }

    #[test]
    fn test_format_drop_table() {
        let sql = "DROP TABLE IF EXISTS users CASCADE";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("DROP TABLE"));
        assert!(formatted.contains("IF EXISTS"));
        assert!(formatted.contains("CASCADE"));
    }

    #[test]
    fn test_format_transaction_begin() {
        let sql = "BEGIN";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert_eq!(formatted, "BEGIN");
    }

    #[test]
    fn test_format_string_escape() {
        let sql = "SELECT 'it''s a test'";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("''"));
    }

    #[test]
    fn test_roundtrip_select() {
        let sql = "SELECT id, name FROM users WHERE status = 'active'";
        let stmt1 = parse_one(sql);
        let formatted = stmt1.to_string();
        let stmt2 = parse_one(&formatted);
        assert_eq!(stmt1, stmt2);
    }

    #[test]
    fn test_roundtrip_insert() {
        let sql = "INSERT INTO users (id, name) VALUES (1, 'Test')";
        let stmt1 = parse_one(sql);
        let formatted = stmt1.to_string();
        let stmt2 = parse_one(&formatted);
        assert_eq!(stmt1, stmt2);
    }

    #[test]
    fn test_format_truncate() {
        let sql = "TRUNCATE TABLE users CASCADE";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("TRUNCATE TABLE"));
        assert!(formatted.contains("CASCADE"));
    }

    #[test]
    fn test_format_create_view() {
        let sql = "CREATE VIEW active_users AS SELECT * FROM users WHERE status = 'active'";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("CREATE VIEW"));
        assert!(formatted.contains("AS SELECT"));
    }

    #[test]
    fn test_format_explain() {
        let sql = "EXPLAIN SELECT * FROM users";
        let stmt = parse_one(sql);
        let formatted = stmt.to_string();
        assert!(formatted.contains("EXPLAIN"));
    }

    #[test]
    fn test_pretty_print_simple_select() {
        let sql = "SELECT id, name FROM users WHERE status = 'active'";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nFROM"));
        assert!(result.contains("\nWHERE"));
        assert!(!result.contains("\nGROUP BY"));
    }

    #[test]
    fn test_pretty_print_complex_query() {
        let sql = "SELECT id, name FROM users WHERE status = 'active' AND age > 18 GROUP BY dept HAVING count(*) > 5 ORDER BY name LIMIT 10";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nFROM"));
        assert!(result.contains("\nWHERE"));
        assert!(result.contains("\nGROUP BY"));
        assert!(result.contains("\nHAVING"));
        assert!(result.contains("\nORDER BY"));
        assert!(result.contains("\nLIMIT"));
    }

    #[test]
    fn test_pretty_print_subquery_from_clause() {
        let sql = "SELECT * FROM (SELECT id FROM t WHERE x = 1) sub";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nFROM (\n"), "outer FROM: {result}");
        assert!(result.contains("\n  FROM t"), "subquery FROM at depth 1: {result}");
        assert!(result.contains("\n  WHERE x = 1"), "subquery WHERE: {result}");
    }

    #[test]
    fn test_pretty_print_exists_subquery() {
        let sql = "SELECT id FROM t WHERE a = 1 AND EXISTS (SELECT 1 FROM u WHERE u.id = t.id)";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\n    FROM u"), "EXISTS subquery FROM: {result}");
        assert!(result.contains("\n    WHERE u.id = t.id"), "EXISTS subquery WHERE: {result}");
    }

    #[test]
    fn test_pretty_print_join_breaks() {
        let sql = "SELECT a.id FROM users a LEFT JOIN orders b ON a.id = b.uid INNER JOIN items c ON b.iid = c.id";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nLEFT JOIN"), "LEFT JOIN on new line: {result}");
        assert!(result.contains("\nINNER JOIN"), "INNER JOIN on new line: {result}");
    }

    #[test]
    fn test_pretty_print_nested_subquery() {
        let sql = "SELECT * FROM (SELECT id FROM (SELECT id FROM deep WHERE x = 1) inner_t) outer_t";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\n  FROM (\n"), "middle subquery at depth 1: {result}");
        assert!(result.contains("\n    FROM deep"), "innermost subquery at depth 2: {result}");
    }

    #[test]
    fn test_pretty_print_and_or_breaks() {
        let sql = "SELECT * FROM t WHERE a = 1 AND b = 2 OR c = 3";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nWHERE"));
        assert!(result.contains("AND"));
        assert!(result.contains("OR"));
        assert!(result.contains('\n'));
    }

    #[test]
    fn test_pretty_print_union() {
        let sql = "SELECT id FROM t1 UNION ALL SELECT id FROM t2";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nUNION ALL"));
    }

    #[test]
    fn test_pretty_print_no_breakpoints_returns_flat() {
        let sql = "INSERT INTO t VALUES (1, 2)";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        let flat_formatter = SqlFormatter::new();
        let flat = flat_formatter.format_statement(&stmt);
        assert_eq!(result, flat);
    }

    #[test]
    fn test_pretty_print_with_indent() {
        let sql = "SELECT id FROM t WHERE x = 1";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true).indent(2);
        let result = formatter.format_statement(&stmt);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() > 1);
        assert!(lines[1].starts_with("    "));
    }

    #[test]
    fn test_pretty_print_default_is_flat() {
        let sql = "SELECT id, name FROM users WHERE status = 'active'";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new();
        let result = formatter.format_statement(&stmt);
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_pretty_print_offset_connect_by() {
        let sql = "SELECT * FROM t WHERE x = 1 OFFSET 5";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nOFFSET"));
    }

    #[test]
    fn test_pretty_print_except_intersect() {
        let sql = "SELECT id FROM t1 EXCEPT SELECT id FROM t2";
        let stmt = parse_one(sql);
        let formatter = SqlFormatter::new().pretty_print(true);
        let result = formatter.format_statement(&stmt);
        assert!(result.contains("\nEXCEPT"));
    }
}
