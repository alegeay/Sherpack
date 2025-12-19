//! AST (Abstract Syntax Tree) for Go templates
//!
//! These structures represent the parsed Go template syntax,
//! which will be transformed into Jinja2 syntax.

use std::fmt;

/// A complete Go template
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub elements: Vec<Element>,
}

/// An element in a template: either raw text or an action
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// Raw text (not inside {{ }})
    RawText(String),
    /// An action (inside {{ }})
    Action(Action),
}

/// An action (directive inside {{ }})
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    /// Whether the action has left whitespace trimming ({{-)
    pub trim_left: bool,
    /// Whether the action has right whitespace trimming (-}})
    pub trim_right: bool,
    /// The action body
    pub body: ActionBody,
}

/// The body of an action
#[derive(Debug, Clone, PartialEq)]
pub enum ActionBody {
    /// Comment: {{/* comment */}}
    Comment(String),
    /// If: {{- if .X }}
    If(Pipeline),
    /// Else if: {{- else if .X }}
    ElseIf(Pipeline),
    /// Else: {{- else }}
    Else,
    /// End: {{- end }}
    End,
    /// Range: {{- range .X }} or {{- range $i, $v := .X }}
    Range {
        /// Optional variable declarations ($i, $v)
        vars: Option<RangeVars>,
        /// The pipeline to iterate over
        pipeline: Pipeline,
    },
    /// With: {{- with .X }}
    With(Pipeline),
    /// Define: {{- define "name" }}
    Define(String),
    /// Template: {{ template "name" . }}
    Template {
        name: String,
        pipeline: Option<Pipeline>,
    },
    /// Block: {{- block "name" . }}
    Block { name: String, pipeline: Pipeline },
    /// A pipeline expression (variable access, function call, etc.)
    Pipeline(Pipeline),
}

/// Variables in a range clause: $i, $v := ...
#[derive(Debug, Clone, PartialEq)]
pub struct RangeVars {
    /// Index variable (optional): $i in `range $i, $v := .X`
    pub index_var: Option<String>,
    /// Value variable: $v in `range $v := .X` or `range $i, $v := .X`
    pub value_var: String,
}

/// A pipeline: a sequence of commands separated by |
#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    /// Optional variable declaration: $x := ...
    pub decl: Option<String>,
    /// The commands in the pipeline
    pub commands: Vec<Command>,
}

impl Pipeline {
    /// Create a simple pipeline with one command
    pub fn simple(cmd: Command) -> Self {
        Self {
            decl: None,
            commands: vec![cmd],
        }
    }

    /// Create a pipeline with a declaration
    pub fn with_decl(var: String, commands: Vec<Command>) -> Self {
        Self {
            decl: Some(var),
            commands,
        }
    }
}

/// A command in a pipeline
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Field access: .Values.x or $.Values.x
    Field(FieldAccess),
    /// Variable: $x
    Variable(String),
    /// Function call: funcName arg1 arg2
    Function { name: String, args: Vec<Argument> },
    /// Literal value
    Literal(Literal),
    /// Parenthesized pipeline
    Parenthesized(Box<Pipeline>),
}

/// Field access: .Values.image.tag
#[derive(Debug, Clone, PartialEq)]
pub struct FieldAccess {
    /// Whether this is a root access ($.Values vs .Values)
    pub is_root: bool,
    /// The path components: ["Values", "image", "tag"]
    pub path: Vec<String>,
}

impl FieldAccess {
    pub fn new(path: Vec<String>) -> Self {
        Self {
            is_root: false,
            path,
        }
    }

    pub fn root(path: Vec<String>) -> Self {
        Self {
            is_root: true,
            path,
        }
    }

    /// Get the full path as a dot-separated string
    pub fn full_path(&self) -> String {
        self.path.join(".")
    }
}

/// An argument to a function
#[derive(Debug, Clone, PartialEq)]
pub enum Argument {
    Field(FieldAccess),
    Variable(String),
    Literal(Literal),
    Pipeline(Box<Pipeline>),
}

/// A literal value
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Char(char),
    Int(i64),
    Float(f64),
    Bool(bool),
    Nil,
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(s) => write!(f, "\"{}\"", s),
            Literal::Char(c) => write!(f, "'{}'", c),
            Literal::Int(n) => write!(f, "{}", n),
            Literal::Float(n) => write!(f, "{}", n),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::Nil => write!(f, "nil"),
        }
    }
}

/// Location in source for error reporting
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_access() {
        let field = FieldAccess::new(vec!["Values".into(), "image".into(), "tag".into()]);
        assert_eq!(field.full_path(), "Values.image.tag");
        assert!(!field.is_root);
    }

    #[test]
    fn test_field_access_root() {
        let field = FieldAccess::root(vec!["Values".into(), "x".into()]);
        assert!(field.is_root);
    }

    #[test]
    fn test_pipeline_simple() {
        let pipeline = Pipeline::simple(Command::Variable("x".into()));
        assert!(pipeline.decl.is_none());
        assert_eq!(pipeline.commands.len(), 1);
    }

    #[test]
    fn test_literal_display() {
        assert_eq!(format!("{}", Literal::String("hello".into())), "\"hello\"");
        assert_eq!(format!("{}", Literal::Int(42)), "42");
        assert_eq!(format!("{}", Literal::Bool(true)), "true");
        assert_eq!(format!("{}", Literal::Nil), "nil");
    }
}
