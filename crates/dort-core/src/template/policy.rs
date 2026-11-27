use super::{ASCII_FILTERS, Issue, LIST_LIMIT, NAMESPACES, PROGRAM, VALUE_LIMIT};
use minijinja::{
    machinery::{Token, ast::*, parse, tokenize},
    syntax::SyntaxConfig,
};
use std::borrow::Cow;

pub(super) const LIST_FILTER: &str = "_dort_list";
const MAX_DEPTH: usize = 16;
const MAX_TAG_TOKENS: usize = 128;
const MAX_TOKENS: usize = 8 * 1024;
type Rejection = (Issue, Option<usize>);

pub(super) fn validate(source: &str, syntax: SyntaxConfig) -> Result<Cow<'_, str>, Rejection> {
    // MiniJinja 3.1.0 adds one to its u16 column on lexer errors.
    let column_limit = usize::from(u16::MAX);
    if source.len() >= column_limit
        && ["{{", "{%", "{#"]
            .iter()
            .any(|start| source.contains(start))
        && let Some(line) = source
            .split('\n')
            .position(|line| line.chars().count() >= column_limit)
    {
        return Err((Issue::SyntaxLimit, Some(line + 1)));
    }
    let for_ends = lexical_limits(source, syntax.clone())?;
    let ast = parse(source, PROGRAM, syntax).map_err(|err| (Issue::JinjaSyntax, err.line()))?;
    let Stmt::Template(template) = ast else {
        unreachable!("parser returns a template")
    };
    check_body(&template.children, 1, false)?;
    if for_ends.is_empty() {
        return Ok(Cow::Borrowed(source));
    }

    // MiniJinja can also iterate strings and maps. Guard each approved loop at runtime.
    let mut lowered =
        String::with_capacity(source.len() + for_ends.len() * (LIST_FILTER.len() + 1));
    let mut start = 0;
    for end in for_ends {
        lowered.push_str(&source[start..end]);
        lowered.push('|');
        lowered.push_str(LIST_FILTER);
        start = end;
    }
    lowered.push_str(&source[start..]);
    Ok(Cow::Owned(lowered))
}

#[derive(Clone, Copy)]
enum Tag {
    Text,
    BlockHead,
    For,
    Expression,
}

enum Block {
    If(usize),
    For,
}

// MiniJinja parses elif chains recursively before the AST can be checked.
fn lexical_limits(source: &str, syntax: SyntaxConfig) -> Result<Vec<usize>, Rejection> {
    let mut tag = Tag::Text;
    let mut groups = Vec::new();
    let mut blocks = Vec::new();
    let mut depth = 0;
    let mut tag_tokens = 0;
    let mut for_ends = Vec::new();
    for (index, entry) in tokenize(source, false, syntax).enumerate() {
        let (token, span) = entry.map_err(|err| (Issue::JinjaSyntax, err.line()))?;
        let line = Some(usize::from(span.start_line));
        tag_tokens += 1;
        if index >= MAX_TOKENS || tag_tokens > MAX_TAG_TOKENS {
            return Err((Issue::SyntaxLimit, line));
        }
        if matches!(tag, Tag::BlockHead) {
            tag = Tag::Expression;
            match token {
                Token::Ident("if") => {
                    blocks.push(Block::If(1));
                    depth += 1;
                }
                Token::Ident("for") => {
                    blocks.push(Block::For);
                    depth += 1;
                    tag = Tag::For;
                }
                Token::Ident("elif") => match blocks.last_mut() {
                    Some(Block::If(cost)) => {
                        *cost += 1;
                        depth += 1;
                    }
                    _ => return Err((Issue::JinjaSyntax, line)),
                },
                Token::Ident("else") if !blocks.is_empty() => (),
                Token::Ident("endif") => match blocks.pop() {
                    Some(Block::If(cost)) => depth -= cost,
                    _ => return Err((Issue::JinjaSyntax, line)),
                },
                Token::Ident("endfor") => match blocks.pop() {
                    Some(Block::For) => depth -= 1,
                    _ => return Err((Issue::JinjaSyntax, line)),
                },
                Token::Ident("else") => return Err((Issue::JinjaSyntax, line)),
                Token::Ident(_) => return Err((Issue::UnsupportedSyntax, line)),
                _ => return Err((Issue::JinjaSyntax, line)),
            }
        } else {
            match token {
                Token::BlockStart => tag = Tag::BlockHead,
                Token::VariableStart => tag = Tag::Expression,
                Token::BlockEnd | Token::VariableEnd | Token::TemplateData(_) => {
                    if matches!(tag, Tag::For) && matches!(token, Token::BlockEnd) {
                        for_ends.push(span.start_offset as usize);
                    }
                    tag = Tag::Text;
                    tag_tokens = 0;
                }
                Token::ParenOpen => groups.push(true),
                Token::BracketOpen => groups.push(false),
                Token::ParenClose | Token::BracketClose => {
                    if groups.pop() != Some(matches!(token, Token::ParenClose)) {
                        return Err((Issue::JinjaSyntax, line));
                    }
                }
                Token::Ident(_)
                | Token::Str(_)
                | Token::String(_)
                | Token::Int(_)
                | Token::Int128(_)
                | Token::Float(_)
                | Token::Eq
                | Token::Ne
                | Token::Dot
                | Token::Comma
                | Token::Pipe => (),
                _ => return Err((Issue::UnsupportedSyntax, line)),
            }
        }
        if depth + groups.len() > MAX_DEPTH {
            return Err((Issue::SyntaxLimit, line));
        }
    }
    Ok(for_ends)
}

fn check_body(body: &[Stmt<'_>], depth: usize, in_loop: bool) -> Result<(), Rejection> {
    for statement in body {
        match statement {
            Stmt::EmitRaw(_) => (),
            Stmt::EmitExpr(node) => check_expr(&node.expr, depth, in_loop)?,
            Stmt::IfCond(node) => {
                check_expr(&node.expr, depth, in_loop)?;
                check_body(&node.true_body, depth + 1, in_loop)?;
                check_body(&node.false_body, depth + 1, in_loop)?;
            }
            Stmt::ForLoop(node) => {
                let reject = (
                    Issue::UnsupportedSyntax,
                    Some(usize::from(node.span().start_line)),
                );
                if node.recursive
                    || node.filter_expr.is_some()
                    || !matches!(&node.target, Expr::Var(var) if !reserved(var.id))
                {
                    return Err(reject);
                }
                match &node.iter {
                    Expr::List(list) if list.items.len() <= LIST_LIMIT => {
                        for item in &list.items {
                            check_expr(item, depth + 1, in_loop)?;
                        }
                    }
                    Expr::List(_) => return Err((Issue::SyntaxLimit, reject.1)),
                    Expr::Var(_) | Expr::GetAttr(_) => check_expr(&node.iter, depth, in_loop)?,
                    _ => return Err(reject),
                }
                check_body(&node.body, depth + 1, true)?;
                check_body(&node.else_body, depth + 1, in_loop)?;
            }
            _ => return Err((Issue::UnsupportedSyntax, None)),
        }
    }
    Ok(())
}

fn reserved(name: &str) -> bool {
    NAMESPACES.contains(&name) || matches!(name, "loop" | "self")
}

fn check_expr(expr: &Expr<'_>, depth: usize, in_loop: bool) -> Result<(), Rejection> {
    let line = Some(usize::from(expr.span().start_line));
    if depth > MAX_DEPTH {
        return Err((Issue::SyntaxLimit, line));
    }
    let child = |expr| check_expr(expr, depth + 1, in_loop);
    let reject = (Issue::UnsupportedSyntax, line);
    match expr {
        Expr::Const(node) => {
            if node
                .value
                .as_str()
                .is_some_and(|text| text.len() > VALUE_LIMIT)
            {
                return Err((Issue::SyntaxLimit, line));
            }
        }
        Expr::Var(node) if !reserved(node.id) => (),
        Expr::GetAttr(node)
            if matches!(&node.expr, Expr::Var(var) if NAMESPACES.contains(&var.id)
            || (in_loop && var.id == "loop" && node.name == "index")) => {}
        Expr::UnaryOp(node) if matches!(node.op, UnaryOpKind::Not) => child(&node.expr)?,
        Expr::BinOp(node)
            if matches!(
                node.op,
                BinOpKind::ScAnd | BinOpKind::ScOr | BinOpKind::Eq | BinOpKind::Ne
            ) =>
        {
            child(&node.left)?;
            child(&node.right)?;
        }
        Expr::Compare(node)
            if node
                .ops
                .iter()
                .all(|op| matches!(op.op, CompareOpKind::Eq | CompareOpKind::Ne)) =>
        {
            child(&node.expr)?;
            for op in &node.ops {
                child(&op.expr)?;
            }
        }
        Expr::Filter(node)
            if node.args.is_empty() && ASCII_FILTERS.iter().any(|(name, _)| *name == node.name) =>
        {
            child(node.expr.as_ref().ok_or(reject)?)?;
        }
        Expr::Test(node) if node.name == "defined" && node.args.is_empty() => child(&node.expr)?,
        _ => return Err(reject),
    }
    Ok(())
}
