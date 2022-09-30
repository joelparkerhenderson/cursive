//! Build views from configuration.
//!
//! ## Recipes
//!
//! Recipes define how to build a view from a json-like config object.
//!
//! It should be easy for third-party view to define a recipe.
//!
//! ## Builders
//!
//! * Users can prepare a builder `Context` to build views, which will collect all available recipes.
//! * They can optionally store named "variables" in the context (callbacks, sizes, ...).
//! * They can then load a configuration (often a yaml file) and render the view in there.
//!
//! ## Details
//!
//! This crate includes:
//! - A public part, always enabled.
//! - An implementation module, conditionally compiled.
use crate::views::BoxedView;

use std::collections::{HashMap, HashSet};

use std::any::Any;

/// Type of a config item.
pub type Config = serde_json::Value;

/// Type of a config object.
pub type Object = serde_json::Map<String, serde_json::Value>;

/// Can build a view from a config.
pub type Builder =
    fn(&serde_json::Value, &mut Context) -> Result<BoxedView, ()>;

/// Can build a wrapper from a config.
pub type WrapperBuilder =
    fn(&serde_json::Value, &mut Context) -> Result<Wrapper, ()>;

/// Can wrap a view.
pub type Wrapper = Box<dyn FnOnce(BoxedView) -> BoxedView>;

/// Everything needed to prepare a view from a config.
/// - Current recipes
/// - Any stored variables/callbacks
pub struct Context {
    recipes: HashMap<String, Builder>,
    wrappers: HashMap<String, WrapperBuilder>,

    variables: HashMap<String, Box<dyn Any>>,
}

/// Error during config parsing.
#[derive(Debug)]
pub enum Error {
    /// A config object was empty.
    EmptyConfig,
    /// A recipe was not found
    RecipeNotFound(String),
    /// A recipe failed to run.
    RecipeFailed,
}

/// Error caused by an invalid config.
pub struct ConfigError {
    /// Variable names present more than once in the config.
    ///
    /// Each variable can only be read once.
    pub duplicate_vars: HashSet<String>,

    /// Variable names not registered in the context before loading the config.
    pub missing_vars: HashSet<String>,
}

impl ConfigError {
    /// Creates a config error if any issue is detected.
    fn from(
        duplicate_vars: HashSet<String>,
        missing_vars: HashSet<String>,
    ) -> Result<(), Self> {
        if duplicate_vars.is_empty() && missing_vars.is_empty() {
            Ok(())
        } else {
            Err(Self {
                duplicate_vars,
                missing_vars,
            })
        }
    }
}

// Parse the given config, and yields all the variable names found.
fn inspect_variables<F: FnMut(&str)>(config: &Config, on_var: &mut F) {
    match config {
        Config::String(name) => {
            if let Some(name) = name.strip_prefix("$") {
                on_var(name);
            }
        }
        Config::Array(array) => {
            for value in array {
                inspect_variables(value, on_var);
            }
        }
        Config::Object(object) => {
            for value in object.values() {
                inspect_variables(value, on_var);
            }
        }
        _ => (),
    }
}

/// Can be created from config
pub trait FromConfig {
    /// Build from a config
    fn from_config(config: &Config) -> Option<Self>
    where
        Self: Sized;
}

impl FromConfig for String {
    fn from_config(config: &Config) -> Option<Self> {
        config.as_str().map(Into::into)
    }
}

impl FromConfig for u64 {
    fn from_config(config: &Config) -> Option<Self> {
        config.as_u64()
    }
}

impl FromConfig for crate::Vec2 {
    fn from_config(config: &Config) -> Option<Self> {
        let x = config[0].as_u64()?;
        let y = config[1].as_u64()?;
        Some(crate::Vec2::new(x as usize, y as usize))
    }
}

impl Context {
    /// Prepare a new context using registered recipes.
    pub fn new() -> Self {
        let recipes = inventory::iter::<Recipe>()
            .map(|recipe| (recipe.name.to_string(), recipe.builder))
            .collect();

        let wrappers = inventory::iter::<WrapperRecipe>()
            .map(|recipe| (recipe.name.to_string(), recipe.builder))
            .collect();
        let variables = HashMap::new();

        Context {
            recipes,
            wrappers,
            variables,
        }
    }

    /// Resolve a value
    pub fn resolve<T: FromConfig + 'static>(
        &mut self,
        config: &Config,
    ) -> Result<T, ()> {
        // First, check if config.get(name) is a string starting with $
        // If so, try to resolve it from variables instead
        if let Some(name) = config.as_str().and_then(|s| s.strip_prefix("$")) {
            self.load(name).map(|b| *b)
        } else {
            T::from_config(config).ok_or(())
        }
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
    pub fn load<T: Any>(&mut self, name: &str) -> Result<Box<T>, ()> {
        self.variables
            .remove(name)
            .ok_or(())
            .and_then(|b| b.downcast().map_err(|_| ()))
    }

    /// Build a wrapper with the given config
    pub fn build_wrapper(
        &mut self,
        config: &Object,
    ) -> Result<Wrapper, Error> {
        // Expect a single key
        let (key, value) =
            config.into_iter().next().ok_or(Error::EmptyConfig)?;

        let recipe = self
            .wrappers
            .get(key)
            .ok_or_else(|| Error::RecipeNotFound(key.into()))?;

        let wrapper =
            (recipe)(value, self).map_err(|_| Error::RecipeFailed)?;

        Ok(wrapper)
    }

    /// Validate a config.
    ///
    /// Returns an error if any variable is missing or used more than once.
    pub fn validate(&self, config: &Config) -> Result<(), ConfigError> {
        let mut vars = HashSet::new();

        let mut duplicates = HashSet::new();

        inspect_variables(config, &mut |variable| {
            if vars.insert(variable.to_string()) {
                // Error! We found a duplicate!
                duplicates.insert(variable.to_string());
            }
        });

        let not_found: HashSet<String> = vars
            .into_iter()
            .filter(|var| !self.variables.contains_key(var))
            .collect();

        ConfigError::from(duplicates, not_found)
    }

    fn get_wrappers(
        &mut self,
        config: &Config,
    ) -> Result<Vec<Wrapper>, Error> {
        fn get_with(config: &Config) -> Option<&Vec<Config>> {
            config.as_object()?["with"].as_array()
        }

        let with = match get_with(config) {
            Some(with) => with,
            None => return Ok(Vec::new()),
        };

        with.into_iter()
            .map(|with| {
                self.build_wrapper(with.as_object().ok_or(Error::EmptyConfig)?)
            })
            .collect()
    }

    /// Build a new view from the given config.
    pub fn build(&mut self, config: &Object) -> Result<BoxedView, Error> {
        // Expect a single key
        let (key, value) =
            config.into_iter().next().ok_or(Error::EmptyConfig)?;

        let with = self.get_wrappers(value)?;

        // This is a simple function, it's copy so doesn't borrow self.
        let recipe = self
            .recipes
            .get(key)
            .ok_or_else(|| Error::RecipeNotFound(key.into()))?;

        let mut view =
            (recipe)(value, self).map_err(|_| Error::RecipeFailed)?;

        // Now, apply optional wrappers
        for wrapper in with {
            view = (wrapper)(view);
        }

        Ok(view)
    }
}

/// Describes how to build a view.
pub struct Recipe {
    /// Name used in config file to use this recipe.
    pub name: &'static str,

    /// Function to run this recipe.
    pub builder: Builder,
}

/// Describes how to build a view wrapper.
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
    (with $name:ident as $t:ty, $builder:expr) => {
        #[cfg(feature = "builder")]
        inventory::submit! {
            $crate::builder::WrapperRecipe {
                name: stringify!($name),
                builder: |config, context| {
                    let builder: fn(&::serde_json::Value, &mut $crate::builder::Context) -> Result<_,()> = $builder;
                    let wrapper = (builder)(config, context)?;

                    Ok(Box::new(move |view| {
                        let view = (wrapper)(view);
                        $crate::views::BoxedView::boxed(view)
                    }))
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
                    let builder: fn(&::serde_json::Value, &mut $crate::builder::Context) -> Result<$t,()> = $builder;
                    (builder)(config, context).map($crate::views::BoxedView::boxed)
                }
            }
        }
    };
    ($t:ty, $builder:expr) => {
        $crate::recipe!($t as stringify!($t), $builder);
    };
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_load() {
        use crate::view::Finder;

        let mut context = crate::builder::Context::new();

        let config = serde_json::json!({
            "TextView": {
                "content": "Foo",
                "with": [
                    {
                        "name": "text"
                    }
                ]
            }
        });

        let mut res = context.build(config.as_object().unwrap()).unwrap();

        let content = res
            .call_on_name("text", |v: &mut crate::views::TextView| {
                v.get_content()
            })
            .unwrap();

        let content = content.source();

        assert_eq!(content, "Foo");
    }
}
