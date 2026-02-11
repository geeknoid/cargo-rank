//! Boolean expression evaluation for filtering crates

use crate::Result;
use cel_interpreter::Program;
use ohno::app_err;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// A boolean expression that can be evaluated against crate metrics
#[derive(Debug, Clone)]
pub struct Expression {
    name: String,
    description: Option<String>,
    points: Option<u32>,
    program: Arc<Program>,

    #[expect(clippy::struct_field_names, reason = "Field name matches struct name intentionally for clarity")]
    expression_string: String,
}

impl Expression {
    /// Create a new expression by parsing an expression string
    ///
    /// # Errors
    /// Returns an error if the expression cannot be parsed
    pub fn new(name: String, description: Option<String>, expression: String, points: Option<u32>) -> Result<Self> {
        let program = Program::compile(&expression).map_err(|e| app_err!("Could not parse expression '{name}': {e}"))?;

        Ok(Self {
            name,
            description,
            points,
            program: Arc::new(program),
            expression_string: expression,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    #[must_use]
    pub const fn points(&self) -> Option<u32> {
        self.points
    }

    #[must_use]
    pub fn expression(&self) -> &str {
        &self.expression_string
    }

    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }
}

impl Serialize for Expression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("Expression", 3)?;
        state.serialize_field("name", &self.name)?;
        if let Some(ref desc) = self.description {
            state.serialize_field("description", desc)?;
        }
        state.serialize_field("expression", &self.expression_string)?;
        if let Some(points) = self.points {
            state.serialize_field("points", &points)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for Expression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ExpressionData {
            name: String,
            description: Option<String>,
            expression: String,
            points: Option<u32>,
        }

        let data = ExpressionData::deserialize(deserializer)?;

        Self::new(data.name, data.description, data.expression, data.points).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_expression() {
        let expr = Expression::new(
            "high_stars".to_string(),
            Some("Checks if repo has many stars".to_string()),
            "stars > 100".to_string(),
            Some(10),
        );

        assert!(expr.is_ok());
        let expr = expr.unwrap();
        assert_eq!(expr.name, "high_stars");
        assert_eq!(expr.description, Some("Checks if repo has many stars".to_string()));
        assert_eq!(expr.expression(), "stars > 100");
        assert_eq!(expr.points, Some(10));
    }

    #[test]
    fn test_create_expression_no_description() {
        let expr = Expression::new("simple_check".to_string(), None, "x > 5".to_string(), None);

        assert!(expr.is_ok());
        let expr = expr.unwrap();
        assert_eq!(expr.name, "simple_check");
        assert!(expr.description.is_none());
    }

    #[test]
    fn test_create_expression_invalid() {
        let expr = Expression::new(
            "bad_expr".to_string(),
            None,
            "(x > 5".to_string(), // Mismatched parentheses
            None,
        );

        let _ = expr.unwrap_err();
    }

    #[test]
    fn test_serialize_deserialize_json() {
        let expr = Expression::new("test_expr".to_string(), None, "a && b".to_string(), None).unwrap();

        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();

        assert_eq!(expr.name, deserialized.name);
        assert_eq!(expr.description, deserialized.description);
        assert_eq!(expr.expression(), deserialized.expression());
    }

    #[test]
    fn test_serialize_with_description_format() {
        let expr = Expression::new("test".to_string(), Some("A test description".to_string()), "x > 5".to_string(), None).unwrap();

        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["description"], "A test description");
        assert_eq!(json["expression"], "x > 5");
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn test_serialize_without_description_format() {
        let expr = Expression::new("test".to_string(), None, "x > 5".to_string(), None).unwrap();

        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["expression"], "x > 5");
        assert!(!json.as_object().unwrap().contains_key("description"));
        assert_eq!(json.as_object().unwrap().len(), 2);
    }

    #[test]
    fn test_deserialize_invalid_expression() {
        let json = r#"{"name": "bad", "expression": "(x > 5"}"#;
        let result: Result<Expression, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Could not parse expression"));
    }

    #[test]
    fn test_deserialize_with_description_invalid_expression() {
        let json = r#"{"name": "bad", "description": "desc", "expression": "!!invalid!!"}"#;
        let result: Result<Expression, _> = serde_json::from_str(json);
        let _ = result.unwrap_err();
    }

    #[test]
    fn test_program_getter() {
        let expr = Expression::new("test".to_string(), None, "x > 5".to_string(), None).unwrap();

        let program = expr.program();
        // Verify it's a valid program by checking it's not null
        assert!(!core::ptr::eq(core::ptr::from_ref::<Program>(program), core::ptr::null()));
    }

    #[test]
    fn test_all_getters() {
        let expr = Expression::new("test_name".to_string(), Some("test_desc".to_string()), "a && b".to_string(), None).unwrap();

        assert_eq!(expr.name(), "test_name");
        assert_eq!(expr.description(), Some("test_desc"));
        assert_eq!(expr.expression(), "a && b");
        assert!(!core::ptr::eq(core::ptr::from_ref::<Program>(expr.program()), core::ptr::null()));
    }

    #[test]
    fn test_roundtrip_with_description() {
        let original = r#"{"name":"test","description":"desc","expression":"x > 5"}"#;
        let expr: Expression = serde_json::from_str(original).unwrap();
        let reserialized = serde_json::to_string(&expr).unwrap();
        let expr2: Expression = serde_json::from_str(&reserialized).unwrap();

        assert_eq!(expr.name(), expr2.name());
        assert_eq!(expr.description(), expr2.description());
        assert_eq!(expr.expression(), expr2.expression());
    }

    #[test]
    fn test_complex_expression() {
        let expr = Expression::new("complex".to_string(), None, "x > 5 && (y < 10 || z == true)".to_string(), None);
        let _ = expr.unwrap();
    }

    #[test]
    fn test_expression_with_functions() {
        let expr = Expression::new("func_test".to_string(), None, "size(mylist) > 0".to_string(), None);
        let _ = expr.unwrap();
    }

    #[test]
    fn test_deserialize_rejects_unknown_fields() {
        let json = r#"{"name": "test", "descriptiono": "typo", "expression": "x > 5"}"#;
        let result: Result<Expression, _> = serde_json::from_str(json);
        assert!(result.is_err(), "misspelled field should be rejected");
    }
}
