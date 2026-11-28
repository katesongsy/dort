use dort_core::template::{
    CONTEXT_LIMIT, Context, Dialect, Input, Issue, LIST_LIMIT, OUTPUT_LIMIT, SOURCE_LIMIT,
    Template, TemplateError, VALUE_LIMIT,
};
use std::{collections::BTreeSet, error::Error};

fn text(value: &str) -> Input {
    Input::Text(value.into())
}

fn inputs<const N: usize>(entries: [(&str, Input); N]) -> Context {
    entries
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect()
}

fn render(dialect: Dialect, source: &str, context: &Context) -> Result<String, TemplateError> {
    Template::compile("sample", dialect, source)?.render(context)
}

fn rejected(dialect: Dialect, source: &str) -> TemplateError {
    Template::compile("sample", dialect, source)
        .err()
        .expect("template must fail to compile")
}

#[test]
fn literal_preserves_whitespace_newlines_and_jinja_collisions() {
    let context = inputs([
        ("name", text("é\r\n\0{{other}}{% endfor %}")),
        ("true", text("literal true")),
        ("False", text("literal False")),
        ("none", text("literal none")),
        ("not", text("literal not")),
        ("loop", text("literal loop")),
        ("host", text("literal host")),
        ("empty", text("")),
    ]);
    let syntax = "{% if true %}{# note #}{% raw %}{% endraw %}{%";
    let source = format!(
        "{syntax}\r\n{{{{true}}}}\t{{{{ False\t}}}} {{{{none}}}}\r{{{{not}}}} {{{{loop}}}} {{{{host}}}} {{{{empty}}}}\n{{{{name}}}}{{{{name}}}}\r\n"
    );
    let expected = format!(
        "{syntax}\r\nliteral true\tliteral False literal none\rliteral not literal loop literal host \né\r\n\0{{{{other}}}}{{% endfor %}}é\r\n\0{{{{other}}}}{{% endfor %}}\r\n"
    );
    let template = Template::compile("sample", Dialect::default(), &source).unwrap();
    assert_eq!(
        template.render(&context).unwrap().as_bytes(),
        expected.as_bytes()
    );
    assert_eq!(
        template.input_names(),
        &BTreeSet::from_iter(
            [
                "name", "true", "False", "none", "not", "loop", "host", "empty"
            ]
            .map(str::to_owned)
        )
    );
    for source in [
        "",
        "\n",
        "\r",
        "\r\n",
        "last line",
        "last line\n\n",
        "standalone {#",
        "stray }}",
    ] {
        assert_eq!(
            render(Dialect::Literal, source, &Context::new()).unwrap(),
            source
        );
    }
}

#[test]
fn literal_rejects_non_legacy_expressions_at_the_opening_line() {
    for expression in [
        "{{}}",
        "{{ name",
        "{{name\n}}",
        "{{name | lower_ascii}}",
        "{{- name -}}",
        "{{ host.os }}",
        "{{é}}",
    ] {
        let error = rejected(Dialect::Literal, &format!("one\r\ntwo\n{expression}"));
        assert_eq!(error.issue, Issue::LiteralSyntax);
        assert_eq!(error.line, Some(3));
    }
    let err = render(Dialect::Literal, "known\n{{missing}}", &Context::new()).unwrap_err();
    assert_eq!((err.issue, err.line), (Issue::MissingInput, Some(2)));
}

#[test]
fn selects_syntax_only_when_requested() {
    let source = "{% if false %}hello{% endif %}{# note #}\n";
    let context = Context::new();
    assert_eq!(render(Dialect::Literal, source, &context).unwrap(), source);
    assert_eq!(render(Dialect::Jinja, source, &context).unwrap(), "\n");
    assert_eq!(
        rejected(Dialect::Jinja, "{% if false %}").issue,
        Issue::JinjaSyntax
    );
}

#[test]
fn jinja_discovers_possible_inputs_and_renders_conditions_loops_and_filters() {
    let source = concat!(
        "{% if host.os is defined and (host.os == 'linux' or not local.disabled) %}",
        "{% for item in local.editors %}{{ loop.index }}={{ item|upper_ascii|lower_ascii }};",
        "{% else %}empty{% endfor %}",
        "{% elif local.true != 'skip' %}{% for item in [local.true, 'B'] %}{{item|upper_ascii}} {% endfor %}",
        "{% else %}{{ missing }}{% endif %}\n",
    );
    let template = Template::compile("editors", Dialect::Jinja, source).unwrap();
    assert_eq!(
        template.input_names(),
        &BTreeSet::from_iter(
            [
                "host.os",
                "local.disabled",
                "local.editors",
                "local.true",
                "missing"
            ]
            .map(str::to_owned)
        )
    );
    let linux = Input::Map(inputs([("os", text("linux"))]));
    let local = Input::Map(inputs([(
        "editors",
        Input::List(vec!["ViM".into(), "ÉMACS".into()]),
    )]));
    assert_eq!(
        template
            .render(&inputs([("host", linux.clone()), ("local", local)]))
            .unwrap(),
        "1=vim;2=Émacs;\n"
    );
    let empty = Input::Map(inputs([("editors", Input::List(vec![]))]));
    assert_eq!(
        template
            .render(&inputs([("host", linux), ("local", empty)]))
            .unwrap(),
        "empty\n"
    );
    let other = Input::Map(inputs([("true", text("a"))]));
    assert_eq!(
        template.render(&inputs([("local", other)])).unwrap(),
        "A B \n"
    );
    let trimmed = "A{% for item in ([\n 'B', '%}']\n) -%}\n{{item}}\n{%- endfor %}C\n";
    assert_eq!(
        render(Dialect::Jinja, trimmed, &Context::new()).unwrap(),
        "AB%}C\n"
    );
}

#[test]
fn jinja_is_strict_on_executed_paths_and_loops_require_lists() {
    let empty = Context::new();
    for source in [
        "{{ unknown }}",
        "{{ unknown|upper_ascii }}",
        "{{ local.unknown }}",
        "{% if unknown %}a{% endif %}",
        "{% if unknown == '' %}a{% endif %}",
        "{% for item in unknown %}a{% endfor %}",
        "{% for item in ['defined', unknown] %}a{% endfor %}",
    ] {
        let err = render(Dialect::Jinja, source, &empty).unwrap_err();
        assert_eq!(err.issue, Issue::MissingInput, "{source}");
    }
    assert_eq!(
        render(
            Dialect::Jinja,
            "{% if local.unknown is not defined %}absent{% else %}{{unknown}}{% endif %}",
            &empty
        )
        .unwrap(),
        "absent"
    );
    let string = inputs([("rows", text("abc"))]);
    let error = render(
        Dialect::Jinja,
        "ignored output\n{% for item in rows %}{{item}}{% endfor %}",
        &string,
    )
    .unwrap_err();
    assert_eq!(error.issue, Issue::InvalidValue);
    assert_eq!(error.line, Some(2));
    assert_eq!(
        render(
            Dialect::Jinja,
            "{% if false %}{% for item in rows %}{{item}}{% endfor %}{% endif %}",
            &string
        )
        .unwrap(),
        ""
    );
}

#[test]
fn rejects_unsupported_constructs_even_when_they_would_not_run() {
    for construct in [
        "{% include '/etc/passwd' %}",
        "{% extends 'base' %}",
        "{% import 'lib' as m %}",
        "{% from 'lib' import f %}",
        "{% block j %}{% endblock %}",
        "{% macro f() %}{% endmacro %}",
        "{% set x = y %}",
        "{% set x %}capture{% endset %}",
        "{% autoescape true %}x{% endautoescape %}",
        "{% filter upper_ascii %}x{% endfilter %}",
        "{{ get_env('HOME') }}",
        "{{ name.upper() }}",
        "{{ range(2000000000) }}",
        "{{ 't' * 2000000000 }}",
        "{{ x + y }}",
        "{{ x ~ y }}",
        "{{ a > b }}",
        "{{ a in rows }}",
        "{{ a if b else c }}",
        "{{ local['key'] }}",
        "{{ x[1:] }}",
        "{{ x|safe }}",
        "{{ x|_dort_list }}",
        "{{ x is string }}",
        "{{ host }}",
        "{{ loop.index }}",
        "{% for a, b in rows %}{% endfor %}",
        "{% for row in rows if ok %}{% endfor %}",
        "{% for row in rows recursive %}{% endfor %}",
        "{% for local in rows %}{% endfor %}",
    ] {
        let source = format!("{{% if false %}}{construct}{{% endif %}}");
        assert_eq!(
            rejected(Dialect::Jinja, &source).issue,
            Issue::UnsupportedSyntax,
            "{construct}"
        );
    }
}

#[test]
fn enforces_exact_source_value_and_output_byte_limits() {
    let empty = Context::new();
    let unicode = "🦀".repeat(SOURCE_LIMIT / 4);
    let value = &unicode[..VALUE_LIMIT];
    for dialect in [Dialect::Literal, Dialect::Jinja] {
        assert_eq!(render(dialect, &unicode, &empty).unwrap(), unicode);
        let ascii = "x".repeat(SOURCE_LIMIT);
        assert_eq!(render(dialect, &ascii, &empty).unwrap(), ascii);
        assert_eq!(
            rejected(dialect, &(unicode.clone() + "x")).issue,
            Issue::SourceLimit
        );
        let context = inputs([("x", text(value))]);
        let source = "{{x}}".repeat(OUTPUT_LIMIT / VALUE_LIMIT);
        assert_eq!(render(dialect, &source, &context).unwrap(), value.repeat(8));
        assert_eq!(
            render(dialect, &(source + "x"), &context)
                .unwrap_err()
                .issue,
            Issue::OutputLimit
        );
        let oversized = inputs([("x", Input::Text(value.to_owned() + "x"))]);
        assert_eq!(
            render(dialect, "{{x}}", &oversized).unwrap_err().issue,
            Issue::ContextLimit
        );
    }
    let mut dense = "a{{x}}".repeat(SOURCE_LIMIT / 6);
    dense.push_str(&"b".repeat(SOURCE_LIMIT - dense.len()));
    let expected = "ax".repeat(SOURCE_LIMIT / 6) + &"b".repeat(SOURCE_LIMIT % 6);
    assert_eq!(
        render(Dialect::Literal, &dense, &inputs([("x", text("x"))])).unwrap(),
        expected
    );
}

#[test]
fn bounds_context_bytes_and_list_length() {
    let mut context = (0..16)
        .map(|i| (format!("v{i:02}"), Input::Text("x".repeat(VALUE_LIMIT - 3))))
        .collect::<Context>();
    assert_eq!(
        context
            .values()
            .map(|value| match value {
                Input::Text(value) => value.len() + 3,
                _ => unreachable!(),
            })
            .sum::<usize>(),
        CONTEXT_LIMIT
    );
    let template = Template::compile("sample", Dialect::Jinja, "ok").unwrap();
    assert_eq!(template.render(&context).unwrap(), "ok");
    context.insert("extra".into(), text(""));
    assert_eq!(
        template.render(&context).unwrap_err().issue,
        Issue::ContextLimit
    );
    let mut many_empty = (0..16 * 1024)
        .map(|i| (format!("v{i}"), text("")))
        .collect::<Context>();
    assert_eq!(template.render(&many_empty).unwrap(), "ok");
    many_empty.insert("extra".into(), text(""));
    assert_eq!(
        template.render(&many_empty).unwrap_err().issue,
        Issue::ContextLimit
    );
    let lists = Template::compile(
        "sample",
        Dialect::Jinja,
        "{% for item in rows %}{{item}}{% endfor %}",
    )
    .unwrap();
    assert_eq!(
        lists
            .render(&inputs([(
                "rows",
                Input::List(vec!["a".into(); LIST_LIMIT])
            )]))
            .unwrap(),
        "a".repeat(LIST_LIMIT)
    );
    assert_eq!(
        lists
            .render(&inputs([(
                "rows",
                Input::List(vec![String::new(); LIST_LIMIT + 1])
            )]))
            .unwrap_err()
            .issue,
        Issue::ContextLimit
    );
    let source = |len| {
        format!(
            "{{% for x in [{}'a'] %}}{{{{x}}}}{{% endfor %}}",
            "'a',".repeat(len - 1)
        )
    };
    assert_eq!(
        render(Dialect::Jinja, &source(LIST_LIMIT), &Context::new()).unwrap(),
        "a".repeat(LIST_LIMIT)
    );
    assert_eq!(
        rejected(Dialect::Jinja, &source(LIST_LIMIT + 1)).issue,
        Issue::SyntaxLimit
    );
}

#[test]
fn bounds_parser_and_runtime_work() {
    for source in [
        "{% if true %}".repeat(4096) + "x",
        "{% if false %}".to_owned() + &"{% elif false %}".repeat(3000),
        "{{".to_owned() + &"(".repeat(10_000),
        "{{ ".to_owned() + &"not ".repeat(10_000) + "true }}",
        "{{ ".to_owned() + &"not ".repeat(120) + "true }}",
        "{{ x".to_owned() + &".x".repeat(10_000) + " }}",
        "{{ x".to_owned() + &"|upper_ascii".repeat(1000) + " }}",
        "{{ x }}".repeat(3000),
        "x".repeat(SOURCE_LIMIT - 2) + "{#",
        "{{ '".to_owned() + &"x".repeat(SOURCE_LIMIT - 4),
    ] {
        assert_eq!(rejected(Dialect::Jinja, &source).issue, Issue::SyntaxLimit);
    }
    for source in [
        "{{ (] }}",
        "{{ ) }}",
        "{% if true %}{% endfor %}",
        "{% else %}",
        "{{ name",
        "{# missing end",
    ] {
        assert_eq!(
            rejected(Dialect::Jinja, source).issue,
            Issue::JinjaSyntax,
            "{source}"
        );
    }
    let source = format!(
        "prefix{}{}",
        "{% for item in rows %}".repeat(4),
        "{% endfor %}".repeat(4)
    );
    let context = inputs([("rows", Input::List(vec!["x".into(); LIST_LIMIT]))]);
    assert_eq!(
        render(Dialect::Jinja, &source, &context).unwrap_err().issue,
        Issue::RenderLimit
    );
}

#[test]
fn errors_carry_identity_line_and_action_but_no_template_or_input_content() {
    let source = "unique-source\n{{ missing }}";
    let context = inputs([("unused", text("unique-context"))]);
    let error = Template::compile("sample\nid", Dialect::Jinja, source)
        .unwrap()
        .render(&context)
        .unwrap_err();
    assert_eq!(error.id, "sample\nid");
    assert_eq!(error.line, Some(2));
    assert_eq!(error.issue, Issue::MissingInput);
    assert!(error.source().is_none());
    let formatted = format!("{error} {error:?}");
    for disallowed in [
        "unique-source",
        "unique-context",
        "{{ missing }}",
        "sample\nid",
    ] {
        assert!(!formatted.contains(disallowed), "{formatted}");
    }
    assert!(formatted.contains("at line 2"));
    assert!(formatted.contains("Supply the missing input"));
}

#[test]
fn checks_sensitive_source_inputs_and_complete_output_in_both_dialects() {
    for dialect in [Dialect::Literal, Dialect::Jinja] {
        assert_eq!(
            rejected(dialect, "password=not-printed").issue,
            Issue::SensitiveSource
        );
        assert_eq!(
            render(
                dialect,
                "visible",
                &inputs([("unused", text("password=not-printed"))])
            )
            .unwrap_err()
            .issue,
            Issue::SensitiveInput
        );
        assert_eq!(
            render(
                dialect,
                "visible\npass{{suffix}}",
                &inputs([("suffix", text("word=not-printed"))])
            )
            .unwrap_err()
            .issue,
            Issue::SensitiveOutput
        );
    }
}
