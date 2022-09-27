//! Build views from configuration.
use crate::views::BoxedView;

use std::collections::HashMap;

use std::any::Any;

/// Type of a config item.
pub type Config = serde_json::Value;

/// Type of a config object.
pub type Object = serde_json::Map<String, serde_json::Value>;

/// Can build a view from a config.
pub type Builder =
    fn(serde_json::Value, &mut Context) -> Result<BoxedView, ()>;

/// Can build a wrapper from a config.
pub type WrapperBuilder =
    fn(serde_json::Value, &mut Context) -> Result<Wrapper, ()>;

/// Can wrap a view.
pub type Wrapper = Box<dyn FnOnce(BoxedView) -> BoxedView>;

/*
/// Foo
pub fn with_modifiers<V: crate::view::View>(
    config: &serde_json::Value,
    view: V,
) -> BoxedView {
    fn _foo<V: crate::view::View>(
        config: &serde_json::Value,
    ) -> Option<impl FnOnce(V) -> BoxedView> {
        let array = config.get("with")?.as_array()?;
        for wrapper in array {}
        None
    }
}
*/

// Everything needed to prepare a view from a config.
// - Current recipes
// - Any stored variables/callbacks
pub struct Context {
    recipes: HashMap<String, Builder>,
    wrappers: HashMap<String, WrapperBuilder>,

    variables: HashMap<String, Box<dyn Any>>,
}

trait FromConfig {
    fn from_config(config: Config) -> Option<Self>
    where
        Self: Sized;
}

impl FromConfig for String {
    fn from_config(config: Config) -> Option<Self> {
        config.as_str().map(Into::into)
    }
}

impl FromConfig for u64 {
    fn from_config(config: Config) -> Option<Self> {
        config.as_u64()
    }
}

impl FromConfig for crate::Vec2 {
    fn from_config(config: Config) -> Option<Self> {
        let x = config[0].as_u64()?;
        let y = config[1].as_u64()?;
        Some(crate::Vec2::new(x, y))
    }
}

impl Context {
    /// Prepare a new context using registered recipes.
    pub fn new() -> Self {
        let recipes = inventory::iter::<Recipe>()
            .map(|recipe| (recipe.name.to_string(), recipe.builder))
            .collect();
        let variables = HashMap::new();

        Context { recipes, variables }
    }

    pub fn resolve<T: FromConfig>(
        &mut self,
        name: &str,
        config: &Object,
    ) -> Option<T> {
        // First, check if config.get(name) is a string starting with $
        // If so, try to resolve it instead
    }

    /// Store a new variable for interpolation.
    ///
    /// Can be a callback, a usize, ...
    pub fn store<S, T>(&mut self, name: S, value: T)
    where
        S: Into<String>,
        T: Any,
    {
        let name = name.into();
        self.variables.insert(name, Box::new(value));
    }

    /// Loads a variable of the given type.
    pub fn load<T: Any>(&mut self, name: &str) -> Option<Box<T>> {
        self.variables.remove(name).and_then(|b| b.downcast().ok())
    }

    /// Build a new view from the given config.
    pub fn build(&mut self, config: Object) -> Result<BoxedView, ()> {
        // Expect a single key
        let (key, mut value) = config.into_iter().next().ok_or(())?;

        let with: Vec<Wrapper> = value
            .as_object_mut()
            .and_then(|o| o.remove("with"))
            .and_then(|with| {
                with.as_array_mut()?.into_iter().map(|with| {}).collect()
            })
            .unwrap_or_else(Vec::new);

        // This is a simple function, it's copy so doesn't borrow self.
        let recipe = self.recipes.get(&key).ok_or(())?;

        let view = (recipe)(value, self);

        // Now, apply optional wrappers

        view
    }
}

/// Describes how to build a view.
pub struct Recipe {
    /// Name used in config file to use this recipe.
    pub name: &'static str,

    /// Function to run this recipe.
    pub builder: Builder,
}

pub struct WrapperRecipe {
    /// Name used in config file to use this wrapper.
    pub name: &'static str,

    /// Function to run this recipe.
    pub builder: WrapperBuilder,
}

inventory::collect!(Recipe);
inventory::collect!(WrapperRecipe);

#[macro_export]
/// Define a recipe to build this view from a config file.
macro_rules! recipe {
    ($t:ty as $name:expr, $builder:expr) => {
        #[cfg(feature = "builder")]
        inventory::submit! {
            $crate::builder::Recipe {
                name: $name,
                builder: |config, context| {
                    let builder: fn(::serde_json::Value, &mut $crate::builder::Context) -> Result<$t,()> = $builder;
                    (builder)(config, context).map($crate::views::BoxedView::boxed)
                }
            }
        }
    };
    ($t:ty as $name:expr, $builder:expr) => {
        #[cfg(feature = "builder")]
        inventory::submit! {
            $crate::builder::Recipe {
                name: $name,
                builder: |config, context| {
                    let builder: fn(::serde_json::Value, &mut $crate::builder::Context) -> Result<$t,()> = $builder;
                    (builder)(config, context).map($crate::views::BoxedView::boxed)
                }
            }
        }
    };
    ($t:ty, $builder:expr) => {
        $crate::recipe!($t as "$t", $builder);
    };
}

/*
pub trait FromConfig: crate::View {
    fn name() -> &'static str;
    fn from_config(config: serde_json::Value) -> Result<Self, ()>
    where
        Self: Sized;
}

impl Recipe {
    pub fn from<T: FromConfig>() -> Self {
        Recipe {
            name: T::name(),
            builder: |config| T::from_config(config).map(BoxedView::boxed),
        }
    }
}
*/

/// Registry of recipes to build views.
#[cfg(feature = "builder")]
pub struct RecipeRegistry {
    recipes: HashMap<String, Recipe>,
}

#[cfg(feature = "builder")]
impl RecipeRegistry {}
