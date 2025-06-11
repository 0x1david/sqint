use rustpython_parser::{Parse, ast};

/// Represents a detected SQL variable
#[derive(Debug, Clone)]
pub struct SqlString {
    file_path: String,
    byte_offset: usize,
    variable_name: String,
    sql_content: String,
    original_sql: String, // Keep original for log printing
}

#[derive(Debug, Clone)]
pub struct DetectionConfig {
    pub variables: Vec<String>,
    pub min_sql_length: usize,
}

pub struct AstSqlDetector {
    config: DetectionConfig,
}

impl AstSqlDetector {
    pub fn new(config: DetectionConfig) -> Self {
        Self { config }
    }

    /// Analyze a Python file and return all detected SQL contexts
    pub fn analyze_file(
        &self,
        file_path: &str,
        source_code: &str,
    ) -> Result<Vec<SqlString>, String> {
        let parsed = ast::Suite::parse(source_code, file_path)
            .map_err(|e| format!("Failed to parse Python file: {}", e))?;

        let mut contexts = Vec::new();
        self.analyze_stmts(&parsed, file_path, &mut contexts);

        Ok(contexts)
    }

    fn analyze_stmts(&self, suite: &ast::Suite, file_path: &str, contexts: &mut Vec<SqlString>) {
        for stmt in suite {
            match stmt {
                ast::Stmt::Assign(assign) => {
                    self.analyze_assignment(assign, file_path, contexts);
                }
                _ => {} // TODO: Add more query detection contexts
            }
        }
    }

    fn analyze_assignment(
        &self,
        assign: &ast::StmtAssign,
        file_path: &str,
        contexts: &mut Vec<SqlString>,
    ) {
        // TODO: Add multi-assignment support
        if assign.targets.len() != 1 {
            return;
        }

        let target = &assign.targets[0];

        if let ast::Expr::Name(name) = target {
            let var_name = &name.id;

            if self.is_sql_variable_name(var_name) {
                if let Some(sql_content) = self.extract_string_content(&assign.value) {
                    let context = SqlString {
                        file_path: file_path.to_string(),
                        byte_offset: assign.range.start().to_usize(),
                        variable_name: var_name.to_string(),
                        original_sql: sql_content.to_string(),
                        sql_content,
                    };
                    contexts.push(context);
                }
            }
        }
    }

    /// Extract string content from an expression (only handles string literals)
    fn extract_string_content(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Constant(constant) => match &constant.value {
                ast::Constant::Str(s) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if variable name suggests it contains SQL
    fn is_sql_variable_name(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.config
            .variables
            .iter()
            .any(|pattern| name_lower.contains(&pattern.to_lowercase()))
    }
}
