use crate::observation::sensitive_content;
use minijinja::{
    Environment, ErrorKind, State, UndefinedBehavior, Value, syntax::SyntaxConfig, value::ValueKind,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, io,
};

mod policy;

pub const SOURCE_LIMIT: usize = 64 * 1024;
pub const VALUE_LIMIT: usize = 16 * 1024;
pub const OUTPUT_LIMIT: usize = 128 * 1024;
pub const CONTEXT_LIMIT: usize = 256 * 1024;
pub const LIST_LIMIT: usize = 32;
const CONTEXT_ITEMS: usize = 16 * 1024;
const PROGRAM: &str = "template";
const LITERAL_PROGRAM: &str = "{% for part in parts %}{{ part }}{% endfor %}";
const NAMESPACES: [&str; 2] = ["host", "local"];
type TextTransform = fn(&str) -> String;
const ASCII_FILTERS: [(&str, TextTransform); 2] = [
    ("upper_ascii", str::to_ascii_uppercase),
    ("lower_ascii", str::to_ascii_lowercase),
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Literal,
    Jinja,
}

pub type Context = BTreeMap<String, Input>;

#[derive(Clone, PartialEq, Eq)]
pub enum Input {
    Text(String),
    List(Vec<String>),
    Map(Context),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Issue {
    #[error("Keep the template within 64 KiB.")]
    SourceLimit,
    #[error("Remove credentials or private keys from the template.")]
    SensitiveSource,
    #[error("Use {{{{ name }}}} with a single input name.")]
    LiteralSyntax,
    #[error("Check the syntax at this line.")]
    JinjaSyntax,
    #[error("Use substitutions, conditions, and simple loops with supported filters and tests.")]
    UnsupportedSyntax,
    #[error("Simplify the template or split it into smaller templates.")]
    SyntaxLimit,
    #[error(
        "Check input names and types. Literal templates take text. Jinja namespace maps may contain text and lists."
    )]
    InvalidContext,
    #[error(
        "Shorten the inputs. Use up to 256 KiB total, 16 KiB per string, and 32 items per list."
    )]
    ContextLimit,
    #[error("Remove credentials or private keys from the inputs.")]
    SensitiveInput,
    #[error("Supply the missing input or check whether it is defined before using it.")]
    MissingInput,
    #[error("Check input types at this line. Loops need lists and text filters need text.")]
    InvalidValue,
    #[error("Reduce the work done by the template. Use fewer or smaller loops.")]
    RenderLimit,
    #[error("Keep the rendered text within 128 KiB.")]
    OutputLimit,
    #[error("Remove credentials or private keys from the rendered text.")]
    SensitiveOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Template {id:?}{location}: {issue}", location = Location(*.line))]
pub struct TemplateError {
    pub id: String,
    pub line: Option<usize>,
    pub issue: Issue,
}

struct Location(Option<usize>);

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.0 {
            write!(f, " at line {line}")?;
        }
        Ok(())
    }
}

pub struct Template<'a> {
    id: &'a str,
    environment: Environment<'a>,
    literal: Option<Vec<Part<'a>>>,
    names: BTreeSet<String>,
}

impl<'a> Template<'a> {
    pub fn compile(id: &'a str, dialect: Dialect, source: &'a str) -> Result<Self, TemplateError> {
        let error = |issue, line| diagnostic(id, issue, line);
        if source.len() > SOURCE_LIMIT {
            return Err(error(Issue::SourceLimit, None));
        }
        if sensitive_content(source.as_bytes()) {
            return Err(error(Issue::SensitiveSource, None));
        }

        let mut environment = environment(dialect);
        let (literal, names) = match dialect {
            Dialect::Literal => {
                let parts = parse_literal(source)
                    .map_err(|line| error(Issue::LiteralSyntax, Some(line)))?;
                let names = parts.iter().filter_map(|part| match part {
                    Part::Text(_) => None,
                    Part::Input { name, .. } => Some((*name).to_owned()),
                });
                let names = names.collect();
                environment
                    .add_template(PROGRAM, LITERAL_PROGRAM)
                    .expect("valid literal renderer");
                (Some(parts), names)
            }
            Dialect::Jinja => {
                let lowered = policy::validate(source, environment.syntax().clone())
                    .map_err(|(issue, line)| error(issue, line))?;
                environment
                    .add_template_owned(PROGRAM, lowered)
                    .map_err(|err| error(Issue::JinjaSyntax, err.line()))?;
                let names = environment
                    .get_template(PROGRAM)
                    .expect("compiled template")
                    .undeclared_variables(true)
                    .into_iter()
                    .collect();
                (None, names)
            }
        };
        Ok(Self {
            id,
            environment,
            literal,
            names,
        })
    }

    /// Possible input references, including references in branches that may not run.
    pub fn input_names(&self) -> &BTreeSet<String> {
        &self.names
    }

    /// Return complete text or a diagnostic. No partial output leaves this method.
    pub fn render(&self, context: &Context) -> Result<String, TemplateError> {
        let error = |issue, line| diagnostic(self.id, issue, line);
        let layer = if self.literal.is_some() {
            Layer::Literal
        } else {
            Layer::Root
        };
        ContextBudget::default()
            .check_map(context, layer)
            .map_err(|issue| error(issue, None))?;
        let context = match &self.literal {
            Some(parts) => {
                literal_context(parts, context).map_err(|(issue, line)| error(issue, line))?
            }
            None => jinja_context(context),
        };
        let template = self
            .environment
            .get_template(PROGRAM)
            .expect("compiled template");
        let mut output = BoundedOutput::default();
        template
            .render_captured_to(context, &mut output)
            .map_err(|err| {
                let issue = match err.kind() {
                    ErrorKind::UndefinedError => Issue::MissingInput,
                    ErrorKind::OutOfFuel => Issue::RenderLimit,
                    ErrorKind::WriteFailure => Issue::OutputLimit,
                    _ => Issue::InvalidValue,
                };
                error(issue, err.line())
            })?;
        if sensitive_content(&output.0) {
            return Err(error(Issue::SensitiveOutput, None));
        }
        Ok(String::from_utf8(output.0).expect("renderer writes UTF-8"))
    }
}

fn diagnostic(id: &str, issue: Issue, line: Option<usize>) -> TemplateError {
    TemplateError {
        id: id.to_owned(),
        line,
        issue,
    }
}

fn environment(dialect: Dialect) -> Environment<'static> {
    let mut environment = Environment::empty();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    environment.set_recursion_limit(32);
    environment.set_syntax(
        SyntaxConfig::builder()
            .keep_trailing_newline(true)
            .build()
            .expect("valid syntax"),
    );
    match dialect {
        Dialect::Literal => environment.set_fuel(Some(512 * 1024)),
        Dialect::Jinja => {
            environment.set_fuel(Some(100_000));
            environment.add_test("defined", |value: Value| !value.is_undefined());
            for (name, transform) in ASCII_FILTERS {
                environment.add_filter(name, move |state: &State, value: Value| {
                    ascii_filter(state, value, transform)
                });
            }
            environment.add_filter(policy::LIST_FILTER, require_list);
        }
    }
    environment
}

fn ascii_filter(
    state: &State,
    value: Value,
    transform: TextTransform,
) -> Result<String, minijinja::Error> {
    if value.is_undefined() {
        return Err(ErrorKind::UndefinedError.into());
    }
    let text = value
        .as_str()
        .filter(|text| text.len() <= VALUE_LIMIT)
        .ok_or(ErrorKind::NonPrimitive)?;
    state.consume_fuel(text.len() as u64)?;
    Ok(transform(text))
}

fn require_list(state: &State, value: Value) -> Result<Value, minijinja::Error> {
    if value.is_undefined() {
        return Err(ErrorKind::UndefinedError.into());
    }
    if value.kind() != ValueKind::Seq {
        return Err(ErrorKind::NonPrimitive.into());
    }
    let len = value
        .len()
        .filter(|&len| len <= LIST_LIMIT)
        .ok_or(ErrorKind::NonPrimitive)?;
    state.consume_fuel(len as u64)?;
    if value.try_iter()?.any(|item| item.is_undefined()) {
        return Err(ErrorKind::UndefinedError.into());
    }
    Ok(value)
}

pub fn is_input_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

enum Part<'a> {
    Text(&'a str),
    Input { name: &'a str, line: usize },
}

fn parse_literal(mut text: &str) -> Result<Vec<Part<'_>>, usize> {
    let mut parts = Vec::new();
    let mut line = 1;
    while let Some((literal, rest)) = text.split_once("{{") {
        line += literal.bytes().filter(|&byte| byte == b'\n').count();
        let (expression, remaining) = rest.split_once("}}").ok_or(line)?;
        let name = expression.trim_matches([' ', '\t']);
        if !is_input_name(name) {
            return Err(line);
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        parts.push(Part::Input { name, line });
        text = remaining;
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    Ok(parts)
}

fn literal_context(parts: &[Part<'_>], context: &Context) -> Result<Value, (Issue, Option<usize>)> {
    let mut size = 0;
    let mut values = Vec::with_capacity(parts.len());
    for part in parts {
        let (text, line) = match *part {
            Part::Text(text) => (text, None),
            Part::Input { name, line } => match context.get(name) {
                Some(Input::Text(text)) => (text.as_str(), Some(line)),
                _ => return Err((Issue::MissingInput, Some(line))),
            },
        };
        size += text.len();
        if size > OUTPUT_LIMIT {
            return Err((Issue::OutputLimit, line));
        }
        if !text.is_empty() {
            values.push(Value::from(text));
        }
    }
    Ok(Value::from_pairs([("parts", Value::from(values))]))
}

#[derive(Default)]
struct ContextBudget {
    bytes: usize,
    items: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Literal,
    Root,
    Namespace,
}

impl ContextBudget {
    fn check_map(&mut self, context: &Context, layer: Layer) -> Result<(), Issue> {
        self.add_items(context.len())?;
        for (name, value) in context {
            self.add_bytes(name.len())?;
            if !is_input_name(name) {
                return Err(Issue::InvalidContext);
            }
            let namespace = layer == Layer::Root && NAMESPACES.contains(&name.as_str());
            match value {
                Input::Text(text) if !namespace => self.text(text)?,
                Input::List(list) if !namespace && layer != Layer::Literal => {
                    if list.len() > LIST_LIMIT {
                        return Err(Issue::ContextLimit);
                    }
                    self.add_items(list.len())?;
                    for item in list {
                        self.text(item)?;
                    }
                }
                Input::Map(map) if namespace => self.check_map(map, Layer::Namespace)?,
                _ => return Err(Issue::InvalidContext),
            }
        }
        Ok(())
    }

    fn text(&mut self, value: &str) -> Result<(), Issue> {
        if value.len() > VALUE_LIMIT {
            return Err(Issue::ContextLimit);
        }
        self.add_bytes(value.len())?;
        if sensitive_content(value.as_bytes()) {
            return Err(Issue::SensitiveInput);
        }
        Ok(())
    }

    fn add_items(&mut self, amount: usize) -> Result<(), Issue> {
        self.items = self.items.saturating_add(amount);
        (self.items <= CONTEXT_ITEMS)
            .then_some(())
            .ok_or(Issue::ContextLimit)
    }

    fn add_bytes(&mut self, amount: usize) -> Result<(), Issue> {
        self.bytes = self.bytes.saturating_add(amount);
        (self.bytes <= CONTEXT_LIMIT)
            .then_some(())
            .ok_or(Issue::ContextLimit)
    }
}

fn jinja_context(context: &Context) -> Value {
    Value::from_pairs(
        context
            .iter()
            .map(|(name, value)| (name.as_str(), engine_value(value)))
            .chain(
                NAMESPACES
                    .into_iter()
                    .filter(|name| !context.contains_key(*name))
                    .map(|name| (name, Value::from(BTreeMap::<String, Value>::new()))),
            ),
    )
}

fn engine_value(input: &Input) -> Value {
    match input {
        Input::Text(text) => Value::from(text.as_str()),
        Input::List(list) => Value::from(
            list.iter()
                .map(|item| Value::from(item.as_str()))
                .collect::<Vec<_>>(),
        ),
        Input::Map(map) => Value::from_pairs(
            map.iter()
                .map(|(key, value)| (key.as_str(), engine_value(value))),
        ),
    }
}

#[derive(Default)]
struct BoundedOutput(Vec<u8>);

impl io::Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > OUTPUT_LIMIT - self.0.len() {
            return Err(io::Error::other("output limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
