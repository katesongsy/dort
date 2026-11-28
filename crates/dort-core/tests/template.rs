use dort_core::template::{
    CONTEXT_LIMIT, Context, Input, Issue, LIST_LIMIT, OUTPUT_LIMIT, SOURCE_LIMIT, Template,
    TemplateError, VALUE_LIMIT,
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

fn render(source: &str, context: &Context) -> Result<String, TemplateError> {
    Template::compile("sample", source)?.render(context)
}

fn rejected(source: &str) -> TemplateError {
    Template::compile("sample", source)
        .err()
        .expect("template must fail to compile")
}

#[test]
fn plain_names_and_the_inputs_namespace_render_the_same_text() {
    let context = inputs([
        ("theme", text("dark")),
        ("true", text("not the constant")),
        ("name", text("é\r\n\0{{other}}{% endfor %}")),
        ("empty", text("")),
    ]);
    let source = "theme = {{ theme }}\nvalue = {{ inputs.theme }}\n{{empty}}|{{ name }}\x20\r\n";
    let template = Template::compile("sample", source).unwrap();
    assert_eq!(
        template.render(&context).unwrap(),
        "theme = dark\nvalue = dark\n|é\r\n\0{{other}}{% endfor %} \r\n"
    );
    assert_eq!(
        template.input_names(),
        &BTreeSet::from_iter(["empty", "inputs.theme", "name", "theme"].map(str::to_owned))
    );
    // Jinja keywords keep their meaning at the root. The namespace reaches a name like that.
    assert_eq!(render("{{ true }}", &context).unwrap(), "True");
    assert_eq!(
        render("{{ inputs.true }}", &context).unwrap(),
        "not the constant"
    );
    for source in [
        "",
        "\n",
        "\r",
        "\r\n",
        "last line",
        "last line\n\n",
        "stray }}",
        "braces { } % %",
    ] {
        assert_eq!(render(source, &Context::new()).unwrap(), source);
    }
}

#[test]
fn host_and_local_values_stay_opt_in() {
    let empty = Context::new();
    for source in ["{{ host.os }}", "{{ local.editor }}"] {
        assert_eq!(
            render(source, &empty).unwrap_err().issue,
            Issue::MissingInput,
            "{source}"
        );
    }
    assert_eq!(rejected("{{ host['os'] }}").issue, Issue::UnsupportedSyntax);
    let context = inputs([
        ("host", Input::Map(inputs([("os", text("linux"))]))),
        ("local", Input::Map(inputs([("editor", text("vim"))]))),
    ]);
    assert_eq!(
        render("{{ host.os }}/{{ local.editor }}", &context).unwrap(),
        "linux/vim"
    );
    assert_eq!(
        render("{{ inputs.os }}", &context).unwrap_err().issue,
        Issue::MissingInput
    );
    assert_eq!(
        render(
            "{% if host.arch is defined %}arch{% else %}none{% endif %}",
            &context
        )
        .unwrap(),
        "none"
    );
}

#[test]
fn a_bare_namespace_fails_with_guidance_that_names_the_qualified_form() {
    for source in [
        "{{ host }}",
        "{{ inputs }}",
        "{{ local }}",
        "{{ host|upper_ascii }}",
        "{% if local %}x{% endif %}",
        "{% for value in host %}x{% endfor %}",
        "{{ inputs == 'x' }}",
    ] {
        let error = rejected(&format!("first\n{source}"));
        assert_eq!(error.issue, Issue::Namespace, "{source}");
        assert_eq!(error.line, Some(2), "{source}");
        let message = error.issue.to_string();
        for qualified in ["host.os", "inputs.theme", "local.editor"] {
            assert!(message.contains(qualified), "{message}");
        }
    }
}

#[test]
fn discovers_possible_inputs_and_renders_conditions_loops_and_filters() {
    let source = concat!(
        "{% if host.os is defined and (host.os == 'linux' or not local.disabled) %}",
        "{% for item in local.editors %}{{ loop.index }}={{ item|upper_ascii|lower_ascii }};",
        "{% else %}empty{% endfor %}",
        "{% elif local.true != 'skip' %}{% for item in [local.true, 'B'] %}{{item|upper_ascii}} {% endfor %}",
        "{% else %}{{ missing }}{% endif %}\n",
    );
    let template = Template::compile("editors", source).unwrap();
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
    assert_eq!(render(trimmed, &Context::new()).unwrap(), "AB%}C\n");
    assert_eq!(
        render("{{ theme }}", &inputs([("theme", text("dark"))])).unwrap(),
        "dark"
    );
}

#[test]
fn is_strict_on_executed_paths_and_loops_require_lists() {
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
        let err = render(source, &empty).unwrap_err();
        assert_eq!(err.issue, Issue::MissingInput, "{source}");
    }
    assert_eq!(
        render(
            "{% if local.unknown is not defined %}absent{% else %}{{unknown}}{% endif %}",
            &empty
        )
        .unwrap(),
        "absent"
    );
    let string = inputs([("rows", text("abc"))]);
    let error = render(
        "ignored output\n{% for item in rows %}{{item}}{% endfor %}",
        &string,
    )
    .unwrap_err();
    assert_eq!(error.issue, Issue::InvalidValue);
    assert_eq!(error.line, Some(2));
    assert_eq!(
        render(
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
        "{{ loop.index }}",
        "{% for a, b in rows %}{% endfor %}",
        "{% for row in rows if ok %}{% endfor %}",
        "{% for row in rows recursive %}{% endfor %}",
        "{% for local in rows %}{% endfor %}",
    ] {
        let source = format!("{{% if false %}}{construct}{{% endif %}}");
        assert_eq!(
            rejected(&source).issue,
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
    assert_eq!(render(&unicode, &empty).unwrap(), unicode);
    let ascii = "x".repeat(SOURCE_LIMIT);
    assert_eq!(render(&ascii, &empty).unwrap(), ascii);
    assert_eq!(rejected(&(unicode.clone() + "x")).issue, Issue::SourceLimit);
    let context = inputs([("x", text(value))]);
    let source = "{{x}}".repeat(OUTPUT_LIMIT / VALUE_LIMIT);
    assert_eq!(render(&source, &context).unwrap(), value.repeat(8));
    assert_eq!(
        render(&(source + "x"), &context).unwrap_err().issue,
        Issue::OutputLimit
    );
    let oversized = inputs([("x", Input::Text(value.to_owned() + "x"))]);
    assert_eq!(
        render("{{x}}", &oversized).unwrap_err().issue,
        Issue::ContextLimit
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
    let template = Template::compile("sample", "ok").unwrap();
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
    let lists = Template::compile("sample", "{% for item in rows %}{{item}}{% endfor %}").unwrap();
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
        render(&source(LIST_LIMIT), &Context::new()).unwrap(),
        "a".repeat(LIST_LIMIT)
    );
    assert_eq!(rejected(&source(LIST_LIMIT + 1)).issue, Issue::SyntaxLimit);
    assert_eq!(
        render(
            "{{ rows|upper_ascii }}",
            &inputs([("rows", Input::List(vec!["a".into()]))])
        )
        .unwrap_err()
        .issue,
        Issue::InvalidValue
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
        assert_eq!(rejected(&source).issue, Issue::SyntaxLimit);
    }
    for source in [
        "{{ (] }}",
        "{{ ) }}",
        "{% if true %}{% endfor %}",
        "{% else %}",
        "{{ name",
        "{# missing end",
    ] {
        assert_eq!(rejected(source).issue, Issue::JinjaSyntax, "{source}");
    }
    let source = format!(
        "prefix{}{}",
        "{% for item in rows %}".repeat(4),
        "{% endfor %}".repeat(4)
    );
    let context = inputs([("rows", Input::List(vec!["x".into(); LIST_LIMIT]))]);
    assert_eq!(
        render(&source, &context).unwrap_err().issue,
        Issue::RenderLimit
    );
}

#[test]
fn errors_carry_identity_line_and_action_but_no_template_or_input_content() {
    let source = "unique-source\n{{ missing }}";
    let context = inputs([("unused", text("unique-context"))]);
    let error = Template::compile("sample\nid", source)
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
fn checks_sensitive_source_inputs_and_complete_output() {
    assert_eq!(
        rejected("password=not-printed").issue,
        Issue::SensitiveSource
    );
    assert_eq!(
        render(
            "visible",
            &inputs([("unused", text("password=not-printed"))])
        )
        .unwrap_err()
        .issue,
        Issue::SensitiveInput
    );
    assert_eq!(
        render(
            "visible\npass{{suffix}}",
            &inputs([("suffix", text("word=not-printed"))])
        )
        .unwrap_err()
        .issue,
        Issue::SensitiveOutput
    );
}
