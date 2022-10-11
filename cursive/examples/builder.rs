use cursive::views::{Dialog, EditView, TextView};

// Let's define some extra recipes based on template files
cursive::recipe!(LabeledField, |config, context| {
    const CONFIG: &str = include_str!("label-view.yaml");
    let template = serde_yaml::from_str(CONFIG).unwrap();
    context.build_template(config, &template)
});

cursive::recipe!(VSpace, |config, context| {
    const CONFIG: &str = include_str!("vspace.yaml");
    let template = serde_yaml::from_str(CONFIG).unwrap();
    context.build_template(config, &template)
});

fn main() {
    // We will build a view from a template (possibly written by another team)
    let mut context = cursive::builder::Context::new();

    // The only thing we need to know are the variables it expects.
    //
    // In our case, it's a title, and an on_edit callback.
    context.store("title", String::from("Config-driven layout example"));
    context.store("quit", Dialog::button_cb(|s| s.quit()));
    context.store(
        "on_edit",
        EditView::on_edit_cb(|s, t, c| {
            s.call_on_name("status", |v: &mut TextView| {
                let spaces: String = std::iter::repeat(" ")
                    .take(c + "You wrote `".len())
                    .collect();
                v.set_content(format!("You wrote `{t}`\n{spaces}^"));
            })
            .unwrap();
        }),
    );

    // Load the template - here it's a yaml file.
    const CONFIG: &str = include_str!("builder.yaml");
    let config = serde_yaml::from_str(CONFIG).unwrap();

    // And build the view
    let view = context.build(&config).unwrap();

    let mut siv = cursive::default();
    siv.screen_mut().add_transparent_layer(view);
    siv.run();
}
