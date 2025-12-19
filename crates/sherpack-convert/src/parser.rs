//! Go template parser
//!
//! Parses Go/Helm template syntax into an AST using pest.

use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;

use crate::ast::*;

#[derive(Parser)]
#[grammar = "go_template.pest"]
struct GoTemplateParser;

/// Parser error
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Parse error: {0}")]
    Pest(Box<pest::error::Error<Rule>>),

    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    #[error("Invalid string: {0}")]
    InvalidString(String),

    #[error("Unexpected rule: {0:?}")]
    UnexpectedRule(Rule),
}

impl From<pest::error::Error<Rule>> for ParseError {
    fn from(e: pest::error::Error<Rule>) -> Self {
        ParseError::Pest(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, ParseError>;

/// Parse a Go template string into an AST
pub fn parse(input: &str) -> Result<Template> {
    let pairs = GoTemplateParser::parse(Rule::template, input)?;

    let mut elements = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::template => {
                for inner in pair.into_inner() {
                    if let Some(elem) = parse_element(inner)? {
                        elements.push(elem);
                    }
                }
            }
            Rule::EOI => {}
            _ => {}
        }
    }

    Ok(Template { elements })
}

fn parse_element(pair: pest::iterators::Pair<Rule>) -> Result<Option<Element>> {
    match pair.as_rule() {
        Rule::raw_text => {
            let text = pair.as_str().to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Element::RawText(text)))
            }
        }
        Rule::action => {
            let action = parse_action(pair)?;
            Ok(Some(Element::Action(action)))
        }
        Rule::EOI => Ok(None),
        _ => Ok(None),
    }
}

fn parse_action(pair: pest::iterators::Pair<Rule>) -> Result<Action> {
    let mut trim_left = false;
    let mut trim_right = false;
    let mut body = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::action_start => {
                trim_left = inner.as_str().ends_with('-');
            }
            Rule::action_end => {
                trim_right = inner.as_str().starts_with('-');
            }
            _ => {
                body = Some(parse_action_body(inner)?);
            }
        }
    }

    Ok(Action {
        trim_left,
        trim_right,
        body: body.unwrap_or(ActionBody::Pipeline(Pipeline {
            decl: None,
            commands: vec![],
        })),
    })
}

fn parse_action_body(pair: pest::iterators::Pair<Rule>) -> Result<ActionBody> {
    match pair.as_rule() {
        Rule::comment => {
            let text = pair.as_str();
            // Remove /* and */
            let content = text
                .strip_prefix("/*")
                .and_then(|s| s.strip_suffix("*/"))
                .unwrap_or(text)
                .to_string();
            Ok(ActionBody::Comment(content))
        }
        Rule::if_action => {
            let pipeline = parse_pipeline_from_inner(pair)?;
            Ok(ActionBody::If(pipeline))
        }
        Rule::else_if_action => {
            let pipeline = parse_pipeline_from_inner(pair)?;
            Ok(ActionBody::ElseIf(pipeline))
        }
        Rule::else_action => Ok(ActionBody::Else),
        Rule::end_action => Ok(ActionBody::End),
        Rule::range_action => {
            let mut vars = None;
            let mut pipeline = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::range_clause => {
                        vars = Some(parse_range_clause(inner)?);
                    }
                    Rule::pipeline | Rule::pipeline_expr => {
                        pipeline = Some(parse_pipeline(inner)?);
                    }
                    _ => {}
                }
            }

            Ok(ActionBody::Range {
                vars,
                pipeline: pipeline.unwrap_or_else(|| Pipeline {
                    decl: None,
                    commands: vec![],
                }),
            })
        }
        Rule::with_action => {
            let pipeline = parse_pipeline_from_inner(pair)?;
            Ok(ActionBody::With(pipeline))
        }
        Rule::define_action => {
            let name = extract_string_literal(pair)?;
            Ok(ActionBody::Define(name))
        }
        Rule::template_action => {
            let mut name = String::new();
            let mut pipeline = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::string_literal => {
                        name = parse_string_literal(inner)?;
                    }
                    Rule::pipeline | Rule::pipeline_expr => {
                        pipeline = Some(parse_pipeline(inner)?);
                    }
                    _ => {}
                }
            }

            Ok(ActionBody::Template { name, pipeline })
        }
        Rule::block_action => {
            let mut name = String::new();
            let mut pipeline = Pipeline {
                decl: None,
                commands: vec![],
            };

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::string_literal => {
                        name = parse_string_literal(inner)?;
                    }
                    Rule::pipeline | Rule::pipeline_expr => {
                        pipeline = parse_pipeline(inner)?;
                    }
                    _ => {}
                }
            }

            Ok(ActionBody::Block { name, pipeline })
        }
        Rule::pipeline | Rule::pipeline_expr | Rule::pipeline_decl => {
            let pipeline = parse_pipeline(pair)?;
            Ok(ActionBody::Pipeline(pipeline))
        }
        _ => {
            // Try to parse as pipeline
            let pipeline = parse_pipeline(pair)?;
            Ok(ActionBody::Pipeline(pipeline))
        }
    }
}

fn parse_pipeline_from_inner(pair: pest::iterators::Pair<Rule>) -> Result<Pipeline> {
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::pipeline | Rule::pipeline_expr | Rule::pipeline_decl => {
                return parse_pipeline(inner);
            }
            _ => {}
        }
    }
    Ok(Pipeline {
        decl: None,
        commands: vec![],
    })
}

fn parse_pipeline(pair: pest::iterators::Pair<Rule>) -> Result<Pipeline> {
    let mut decl = None;
    let mut commands = Vec::new();

    match pair.as_rule() {
        Rule::pipeline_decl => {
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::variable => {
                        decl = Some(inner.as_str().trim_start_matches('$').to_string());
                    }
                    Rule::pipeline_expr => {
                        let sub = parse_pipeline(inner)?;
                        commands = sub.commands;
                    }
                    _ => {}
                }
            }
        }
        Rule::pipeline | Rule::pipeline_expr => {
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command => {
                        commands.push(parse_command(inner)?);
                    }
                    Rule::pipeline_decl => {
                        let sub = parse_pipeline(inner)?;
                        decl = sub.decl;
                        commands.extend(sub.commands);
                    }
                    Rule::pipeline_expr => {
                        let sub = parse_pipeline(inner)?;
                        commands.extend(sub.commands);
                    }
                    _ => {
                        // Try parsing as command directly
                        if let Ok(cmd) = parse_command(inner) {
                            commands.push(cmd);
                        }
                    }
                }
            }
        }
        _ => {
            // Try to parse as a single command
            commands.push(parse_command(pair)?);
        }
    }

    Ok(Pipeline { decl, commands })
}

fn parse_command(pair: pest::iterators::Pair<Rule>) -> Result<Command> {
    match pair.as_rule() {
        Rule::command => {
            // Unwrap the command and parse its inner content
            if let Some(inner) = pair.into_inner().next() {
                return parse_command(inner);
            }
            Err(ParseError::UnexpectedRule(Rule::command))
        }
        Rule::parenthesized => {
            for inner in pair.into_inner() {
                if matches!(
                    inner.as_rule(),
                    Rule::pipeline | Rule::pipeline_expr | Rule::pipeline_decl
                ) {
                    let pipeline = parse_pipeline(inner)?;
                    return Ok(Command::Parenthesized(Box::new(pipeline)));
                }
            }
            Err(ParseError::UnexpectedRule(Rule::parenthesized))
        }
        Rule::function_call => {
            let mut name = String::new();
            let mut args = Vec::new();

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::identifier => {
                        name = inner.as_str().to_string();
                    }
                    Rule::argument => {
                        args.push(parse_argument(inner)?);
                    }
                    _ => {}
                }
            }

            Ok(Command::Function { name, args })
        }
        Rule::method_call => {
            let mut field = None;
            let mut args = Vec::new();

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::field_chain => {
                        field = Some(parse_field_chain(inner)?);
                    }
                    Rule::argument => {
                        args.push(parse_argument(inner)?);
                    }
                    _ => {}
                }
            }

            // Convert method call to function call
            if let Some(f) = field {
                let method_name = f.path.last().cloned().unwrap_or_default();
                if args.is_empty() {
                    Ok(Command::Field(f))
                } else {
                    Ok(Command::Function {
                        name: method_name,
                        args,
                    })
                }
            } else {
                Err(ParseError::UnexpectedRule(Rule::method_call))
            }
        }
        Rule::field_chain => {
            let field = parse_field_chain(pair)?;
            Ok(Command::Field(field))
        }
        Rule::variable => {
            let name = pair.as_str().trim_start_matches('$').to_string();
            Ok(Command::Variable(name))
        }
        Rule::literal => {
            let lit = parse_literal(pair)?;
            Ok(Command::Literal(lit))
        }
        Rule::identifier | Rule::bare_identifier => {
            // Bare identifier is a function call with no args (like "now" or "fail" or "quote")
            let name = match pair.as_rule() {
                Rule::bare_identifier => {
                    pair.into_inner()
                        .next()
                        .map(|p| p.as_str().to_string())
                        .unwrap_or_default()
                }
                _ => pair.as_str().to_string(),
            };
            Ok(Command::Function { name, args: vec![] })
        }
        Rule::string_literal | Rule::number | Rule::boolean | Rule::nil => {
            let lit = parse_literal(pair)?;
            Ok(Command::Literal(lit))
        }
        _ => Err(ParseError::UnexpectedRule(pair.as_rule())),
    }
}

fn parse_field_chain(pair: pest::iterators::Pair<Rule>) -> Result<FieldAccess> {
    let text = pair.as_str();

    // Check for root marker ($.)
    let is_root = text.starts_with("$.");

    // Remove leading $. or .
    let path_str = text
        .trim_start_matches("$.")
        .trim_start_matches('$')
        .trim_start_matches('.');

    // Split by dots
    let path: Vec<String> = path_str
        .split('.')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    Ok(FieldAccess { is_root, path })
}

fn parse_argument(pair: pest::iterators::Pair<Rule>) -> Result<Argument> {
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::field_chain => {
                let field = parse_field_chain(inner)?;
                return Ok(Argument::Field(field));
            }
            Rule::variable => {
                let name = inner.as_str().trim_start_matches('$').to_string();
                return Ok(Argument::Variable(name));
            }
            Rule::literal | Rule::string_literal | Rule::number | Rule::boolean | Rule::nil => {
                let lit = parse_literal(inner)?;
                return Ok(Argument::Literal(lit));
            }
            Rule::parenthesized | Rule::pipeline | Rule::pipeline_expr => {
                let pipeline = parse_pipeline(inner)?;
                return Ok(Argument::Pipeline(Box::new(pipeline)));
            }
            _ => {}
        }
    }
    Err(ParseError::UnexpectedRule(Rule::argument))
}

fn parse_literal(pair: pest::iterators::Pair<Rule>) -> Result<Literal> {
    match pair.as_rule() {
        Rule::literal => {
            if let Some(inner) = pair.into_inner().next() {
                return parse_literal(inner);
            }
            Err(ParseError::UnexpectedRule(Rule::literal))
        }
        Rule::string_literal => {
            let s = parse_string_literal(pair)?;
            Ok(Literal::String(s))
        }
        Rule::char_literal => {
            let text = pair.as_str();
            let c = text
                .trim_start_matches('\'')
                .trim_end_matches('\'')
                .chars()
                .next()
                .unwrap_or(' ');
            Ok(Literal::Char(c))
        }
        Rule::number => {
            let text = pair.as_str();
            if text.contains('.') || text.contains('e') || text.contains('E') {
                let n: f64 = text
                    .parse()
                    .map_err(|_| ParseError::InvalidNumber(text.to_string()))?;
                Ok(Literal::Float(n))
            } else if text.starts_with("0x") || text.starts_with("0X") {
                let n = i64::from_str_radix(&text[2..], 16)
                    .map_err(|_| ParseError::InvalidNumber(text.to_string()))?;
                Ok(Literal::Int(n))
            } else if text.starts_with("0o") || text.starts_with("0O") {
                let n = i64::from_str_radix(&text[2..], 8)
                    .map_err(|_| ParseError::InvalidNumber(text.to_string()))?;
                Ok(Literal::Int(n))
            } else if text.starts_with("0b") || text.starts_with("0B") {
                let n = i64::from_str_radix(&text[2..], 2)
                    .map_err(|_| ParseError::InvalidNumber(text.to_string()))?;
                Ok(Literal::Int(n))
            } else {
                let n: i64 = text
                    .parse()
                    .map_err(|_| ParseError::InvalidNumber(text.to_string()))?;
                Ok(Literal::Int(n))
            }
        }
        Rule::boolean => {
            let b = pair.as_str() == "true";
            Ok(Literal::Bool(b))
        }
        Rule::nil => Ok(Literal::Nil),
        _ => Err(ParseError::UnexpectedRule(pair.as_rule())),
    }
}

fn parse_string_literal(pair: pest::iterators::Pair<Rule>) -> Result<String> {
    let text = pair.as_str();

    // Handle backtick strings
    if text.starts_with('`') {
        return Ok(text.trim_matches('`').to_string());
    }

    // Handle quoted strings
    let inner = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(text);

    // Process escape sequences
    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('\'') => result.push('\''),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

fn extract_string_literal(pair: pest::iterators::Pair<Rule>) -> Result<String> {
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::string_literal {
            return parse_string_literal(inner);
        }
    }
    Err(ParseError::InvalidString("No string literal found".to_string()))
}

fn parse_range_clause(pair: pest::iterators::Pair<Rule>) -> Result<RangeVars> {
    let mut vars = Vec::new();

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::range_vars {
            for var in inner.into_inner() {
                if var.as_rule() == Rule::variable {
                    vars.push(var.as_str().trim_start_matches('$').to_string());
                }
            }
        }
    }

    match vars.len() {
        0 => Ok(RangeVars {
            index_var: None,
            value_var: "item".to_string(),
        }),
        1 => Ok(RangeVars {
            index_var: None,
            value_var: vars.remove(0),
        }),
        _ => Ok(RangeVars {
            index_var: Some(vars.remove(0)),
            value_var: vars.remove(0),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_variable() {
        let result = parse("{{ .Values.name }}").unwrap();
        assert_eq!(result.elements.len(), 1);

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Pipeline(pipeline) = &action.body {
                assert_eq!(pipeline.commands.len(), 1);
                if let Command::Field(field) = &pipeline.commands[0] {
                    assert_eq!(field.path, vec!["Values", "name"]);
                }
            }
        }
    }

    #[test]
    fn test_parse_with_trim() {
        let result = parse("{{- .Values.name -}}").unwrap();
        if let Element::Action(action) = &result.elements[0] {
            assert!(action.trim_left);
            assert!(action.trim_right);
        }
    }

    #[test]
    fn test_parse_if() {
        let result = parse("{{- if .Values.enabled }}yes{{- end }}").unwrap();
        assert_eq!(result.elements.len(), 3);

        if let Element::Action(action) = &result.elements[0] {
            assert!(matches!(action.body, ActionBody::If(_)));
        }
    }

    #[test]
    fn test_parse_range() {
        let result = parse("{{- range .Values.items }}{{ . }}{{- end }}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Range { vars, pipeline } = &action.body {
                assert!(vars.is_none());
                assert!(!pipeline.commands.is_empty());
            }
        }
    }

    #[test]
    fn test_parse_range_with_vars() {
        let result = parse("{{- range $i, $v := .Values.items }}{{ $v }}{{- end }}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Range { vars, .. } = &action.body {
                let vars = vars.as_ref().unwrap();
                assert_eq!(vars.index_var, Some("i".to_string()));
                assert_eq!(vars.value_var, "v");
            }
        }
    }

    #[test]
    fn test_parse_pipeline() {
        let result = parse("{{ .Values.name | quote }}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Pipeline(pipeline) = &action.body {
                assert_eq!(pipeline.commands.len(), 2);
            }
        }
    }

    #[test]
    fn test_parse_function_call() {
        let result = parse("{{ printf \"%s-%s\" .Release.Name .Chart.Name }}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Pipeline(pipeline) = &action.body {
                if let Command::Function { name, args } = &pipeline.commands[0] {
                    assert_eq!(name, "printf");
                    assert_eq!(args.len(), 3);
                }
            }
        }
    }

    #[test]
    fn test_parse_define() {
        let result = parse("{{- define \"myapp.name\" -}}test{{- end }}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Define(name) = &action.body {
                assert_eq!(name, "myapp.name");
            }
        }
    }

    #[test]
    fn test_parse_include() {
        let result = parse("{{ include \"myapp.name\" . }}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Pipeline(pipeline) = &action.body {
                if let Command::Function { name, args } = &pipeline.commands[0] {
                    assert_eq!(name, "include");
                    assert_eq!(args.len(), 2);
                }
            }
        }
    }

    #[test]
    fn test_parse_comment() {
        let result = parse("{{/* This is a comment */}}").unwrap();

        if let Element::Action(action) = &result.elements[0] {
            if let ActionBody::Comment(text) = &action.body {
                assert_eq!(text.trim(), "This is a comment");
            }
        }
    }

    #[test]
    fn test_parse_raw_text() {
        let result = parse("apiVersion: v1\nkind: ConfigMap").unwrap();
        assert_eq!(result.elements.len(), 1);

        if let Element::RawText(text) = &result.elements[0] {
            assert!(text.contains("apiVersion: v1"));
        }
    }

    #[test]
    fn test_parse_nested_boolean() {
        // Simple and
        let result = parse("{{ and .Values.a .Values.b }}");
        assert!(result.is_ok(), "Simple and failed: {:?}", result);

        // And with parenthesized eq
        let result = parse("{{ and (eq .Values.a \"x\") .Values.b }}");
        assert!(result.is_ok(), "And with eq failed: {:?}", result);

        // Full nested
        let result = parse("{{- if and (eq .Values.a \"x\") (or .Values.b .Values.c) }}ok{{- end }}");
        assert!(result.is_ok(), "Full nested failed: {:?}", result);
    }

    #[test]
    fn test_parse_parenthesized_function() {
        let result = parse("{{ (eq .Values.a \"x\") }}");
        assert!(result.is_ok(), "Parenthesized eq failed: {:?}", result);
    }
}
