use cursive::views::{EditView, TextView};

// Package a config into a new recipe!
cursive::recipe!(MyDialog, |config, context| {
    const CONFIG: &str = include_str!("builder.yaml");
    let config = serde_yaml::from_str(MY_DIALOG_CONFIG).unwrap();

    context
        .sub_context(|c| c.store("foo", config))
        .build(&config)
});

fn main() {
    let mut context = cursive::builder::Context::new();

    context.store("title", String::from("Config-driven layout example"));

    context.store(
        "on_edit",
        EditView::on_edit_cb(|s, t, c| {
            s.call_on_name("status", |v: &mut TextView| {
                v.set_content(format!("Editing {t} @ {c}."));
            })
            .unwrap();
        }),
    );

    let config = serde_yaml::from_str(config).unwrap();

    let res = context.build(&config).unwrap();

    let mut siv = cursive::default();
    siv.screen_mut().add_transparent_layer(res);
    siv.run();
}
