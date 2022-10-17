use cursive::views::{EditView, TextView};

// This is how we can define some global recipes.
// Here, we define a recipe from a template.
cursive::recipe!(LabeledField from {
    serde_yaml::from_str(include_str!("label-view.yaml")).unwrap()
});

cursive::recipe!(VSpace from {
    serde_yaml::from_str(include_str!("vspace.yaml")).unwrap()
});

// We can also define recipe that build arbitrary views.
cursive::recipe!(Titled, |config, context| {
    use cursive::views::Panel;

    // Fetch a string from the config
    let title: String = context.resolve(&config["title"])?;

    // Build a view from the other field
    let child = context.build(&config["child"])?;

    // And return some view
    Ok(Panel::new(child).title(title))
});

fn main() {
    // We will build a view from a template (possibly written by another team)
    let mut context = cursive::builder::Context::new();

    // The only thing we need to know are the variables it expects.
    //
    // In our case, it's a title, and an on_edit callback.
    context.store("title", String::from("Config-driven layout example"));
    context.store("on_edit", EditView::on_edit_cb(on_edit_callback));

    // Load the template - here it's a yaml file.
    const CONFIG: &str = include_str!("builder.yaml");
    let config = serde_yaml::from_str(CONFIG).unwrap();

    // And build the view
    let view = context.build(&config).unwrap();

    let mut siv = cursive::default();
    siv.screen_mut().add_transparent_layer(view);
    siv.run();
}

// Just a regular callback for EditView::on_edit
fn on_edit_callback(siv: &mut cursive::Cursive, text: &str, cursor: usize) {
    siv.call_on_name("status", |v: &mut TextView| {
        let spaces: String = std::iter::repeat(" ")
            .take(cursor + "You wrote `".len())
            .collect();
        v.set_content(format!("You wrote `{text}`\n{spaces}^"));
    })
    .unwrap();
}

// Still TODO:
// * !! Auto-generate callback helpers (proc macro?)
// * Write more recipes
// * Simplify a bit Rc everywhere
// * resolve() could return a `Result<Option<T>, Error>` to treat missing and invalid type differently?
// * Merge recipes & variables?
// * Documentation
