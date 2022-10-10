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
use std::rc::Rc;

use std::any::Any;

/// Type of a config item.
pub type Config = serde_json::Value;

/// Type of a config object.
pub type Object = serde_json::Map<String, serde_json::Value>;

/// Can build a view from a config.
pub type Builder =
    fn(&serde_json::Value, &Context) -> Result<BoxedView, Error>;

/// Can build a wrapper from a config.
pub type WrapperBuilder =
    fn(&serde_json::Value, &Context) -> Result<Wrapper, Error>;

/// Can wrap a view.
pub type Wrapper = Box<dyn FnOnce(BoxedView) -> BoxedView>;

/// Everything needed to prepare a view from a config.
/// - Current recipes
/// - Any stored variables/callbacks
pub struct Context {
    variables: Rc<Variables>,
    recipes: Rc<Recipes>,
}

struct Recipes {
    recipes: HashMap<String, Builder>,
    wrappers: HashMap<String, WrapperBuilder>,
}

struct Variables {
    variables: HashMap<String, Box<dyn Any>>,

    parent: Option<Rc<Variables>>,
}

trait FnOneArg {
    type Arg<'a>;
    type Output;

    fn call<'a, 'b>(&'a self, a: Self::Arg<'b>) -> Self::Output;
}

/// Error during config parsing.
#[derive(Debug)]
pub enum Error {
    /// The configuration was invalid.
    InvalidConfig {
        /// Description of the issue
        message: String,
        /// Optional offending config object
        config: Config,
    },

    /// Found no variable with the given name.
    NoSuchVariable(String),

    /// Found a variable, but with a different type than expected.
    IncorrectVariableType {
        /// Name of the offending variable
        name: String,
        /// Expected type
        expected_type: String,
    },

    /// Could not load the given config.
    CouldNotLoad {
        /// Expected type
        expected_type: String,
        /// Config value that could not be parsed
        config: Config,
    },

    /// A recipe was not found
    RecipeNotFound(String),

    /// A recipe failed to run.
    RecipeFailed(String, Box<Error>),
}

/// Error caused by an invalid config.
#[derive(Debug)]
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
            if let Some(name) = name.strip_prefix('$') {
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

impl FromConfig for usize {
    fn from_config(config: &Config) -> Option<Self> {
        config.as_u64().map(|v| v as usize)
    }
}

impl FromConfig for crate::Vec2 {
    fn from_config(config: &Config) -> Option<Self> {
        let x = config[0].as_u64()?;
        let y = config[1].as_u64()?;
        Some(crate::Vec2::new(x as usize, y as usize))
    }
}

impl FromConfig for crate::direction::Orientation {
    fn from_config(config: &Config) -> Option<Self> {
        let config = config.as_str()?;
        Some(match config {
            "vertical" => Self::Vertical,
            "horizontal" => Self::Horizontal,
            _ => return None,
        })
    }
}

impl FromConfig for crate::view::Margins {
    fn from_config(config: &Config) -> Option<Self> {
        Some(match config {
            Config::Object(config) => Self::lrtb(
                config.get("left")?.as_u64()? as usize,
                config.get("right")?.as_u64()? as usize,
                config.get("top")?.as_u64()? as usize,
                config.get("bottom")?.as_u64()? as usize,
            ),
            Config::Number(n) => {
                let n = n.as_u64()? as usize;
                Self::lrtb(n, n, n, n)
            }
            _ => return None,
        })
    }
}

new_default!(Context);

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

        let recipes = Rc::new(Recipes { recipes, wrappers });

        let variables = Rc::new(Variables {
            variables,
            parent: None,
        });

        Self { recipes, variables }
    }

    /// Resolve a value.
    ///
    /// Needs to be a reference to a variable.
    pub fn resolve_as_var<T: Clone + 'static>(
        &self,
        config: &Config,
    ) -> Result<T, Error> {
        self.variables.resolve_as_var(config)
    }

    /// Resolve a value
    pub fn resolve<T: FromConfig + Clone + 'static>(
        &self,
        config: &Config,
    ) -> Result<T, Error> {
        self.variables.resolve(config)
    }

    /// Store a new variable for interpolation.
    ///
    /// Can be a callback, a usize, ...
    pub fn store<S, T>(&mut self, name: S, value: T)
    where
        S: Into<String>,
        T: Any,
    {
        if let Some(variables) = Rc::get_mut(&mut self.variables) {
            variables.store(name, value);
        }
    }

    /// Foo
    pub fn store_config<S>(&mut self, name: S, value: &Config)
    where
        S: Into<String>,
    {
        match value {
            Config::String(value) => self.store(name, value.clone()),
            Config::Bool(value) => self.store(name, *value),
            Config::Number(value) => {
                if let Some(value) = value.as_u64() {
                    self.store(name, value);
                } else if let Some(value) = value.as_f64() {
                    self.store(name, value);
                }
            }
            other => self.store(name, other.clone()),
        }
    }

    /// Loads a variable of the given type.
    pub fn load<T: Any + Clone>(&self, name: &str) -> Result<T, Error> {
        self.variables.load(name)
    }

    /// Build a wrapper with the given config
    pub fn build_wrapper(&self, config: &Config) -> Result<Wrapper, Error> {
        // Expect a single key
        let (key, value) = match config {
            Config::String(key) => (key, &Config::Null),
            Config::Object(config) => {
                config.into_iter().next().ok_or(Error::InvalidConfig {
                    message: "Expected non-empty object".into(),
                    config: config.clone().into(),
                })?
            }
            _ => {
                return Err(Error::InvalidConfig {
                    message: "Expected string or object".into(),
                    config: config.clone(),
                })
            }
        };

        let recipe = self
            .recipes
            .wrappers
            .get(key)
            .ok_or_else(|| Error::RecipeNotFound(key.into()))?;

        let wrapper = (recipe)(value, self)
            .map_err(|e| Error::RecipeFailed(key.into(), Box::new(e)))?;

        Ok(wrapper)
    }

    /// Validate a config.
    ///
    /// Returns an error if any variable is missing or used more than once.
    pub fn validate(&self, config: &Config) -> Result<(), ConfigError> {
        let mut vars = HashSet::new();

        let mut duplicates = HashSet::new();

        inspect_variables(config, &mut |variable| {
            if !vars.insert(variable.to_string()) {
                // Error! We found a duplicate!
                duplicates.insert(variable.to_string());
            }
        });

        let not_found: HashSet<String> = vars
            .into_iter()
            .filter(|var| !self.variables.variables.contains_key(var))
            .collect();

        ConfigError::from(duplicates, not_found)
    }

    fn get_wrappers(&self, config: &Config) -> Result<Vec<Wrapper>, Error> {
        fn get_with(config: &Config) -> Option<&Vec<Config>> {
            config.as_object()?.get("with")?.as_array()
        }

        let with = match get_with(config) {
            Some(with) => with,
            None => return Ok(Vec::new()),
        };

        with.iter().map(|with| self.build_wrapper(with)).collect()
    }

    /// Build a new view from the given config.
    pub fn build(&self, config: &Config) -> Result<BoxedView, Error> {
        let (key, value) = match config {
            // Some views can be built from a null config.
            Config::String(name) => (name, &serde_json::Value::Null),
            // Most view require a full object.
            Config::Object(config) => {
                // Expect a single key
                config.iter().next().ok_or(Error::InvalidConfig {
                    message: "Expected non-empty object".into(),
                    config: config.clone().into(),
                })?
            }
            _ => {
                return Err(Error::InvalidConfig {
                    message: "Expected object or string.".into(),
                    config: config.clone(),
                })
            }
        };

        let with = self.get_wrappers(value)?;

        // This is a simple function, it's copy so doesn't borrow self.
        let recipe = self
            .recipes
            .recipes
            .get(key)
            .ok_or_else(|| Error::RecipeNotFound(key.into()))?;

        let mut view = (recipe)(value, self)
            .map_err(|e| Error::RecipeFailed(key.into(), Box::new(e)))?;

        // Now, apply optional wrappers
        for wrapper in with {
            view = (wrapper)(view);
        }

        Ok(view)
    }

    /// Prepare a new context with some variable overrides.
    pub fn sub_context<F>(&self, f: F) -> Context
    where
        F: FnOnce(&mut Context),
    {
        let variables = Rc::new(Variables {
            variables: HashMap::new(),
            parent: Some(Rc::clone(&self.variables)),
        });

        let recipes = Rc::clone(&self.recipes);

        let mut context = Context { recipes, variables };
        f(&mut context);
        context
    }

    /// Bar
    pub fn build_template(
        &self,
        config: &Config,
        template: &Config,
    ) -> Result<BoxedView, Error> {
        let res = self
            .sub_context(|c| {
                if let Some(config) = config.as_object() {
                    for (key, value) in config.iter() {
                        c.store_config(key, value);
                    }
                }
            })
            .build(template)?;

        Ok(res)
    }
}

impl Variables {
    /// Resolve a value.
    ///
    /// Needs to be a reference to a variable.
    pub fn resolve_as_var<T: Clone + 'static>(
        &self,
        config: &Config,
    ) -> Result<T, Error> {
        if let Some(name) = config.as_str().and_then(|s| s.strip_prefix('$')) {
            self.load(name)
        } else {
            Err(Error::CouldNotLoad {
                expected_type: std::any::type_name::<T>().into(),
                config: config.clone(),
            })
        }
    }

    /// Resolve a value
    pub fn resolve<T: FromConfig + Clone + 'static>(
        &self,
        config: &Config,
    ) -> Result<T, Error> {
        // First, check if config.get(name) is a string starting with $
        // If so, try to resolve it from variables instead
        if let Some(name) = config.as_str().and_then(|s| s.strip_prefix('$')) {
            self.load(name)
        } else {
            T::from_config(config).ok_or_else(|| Error::CouldNotLoad {
                expected_type: std::any::type_name::<T>().into(),
                config: config.clone(),
            })
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
        // eprintln!(
        //     "Storing {name} with type {} (ID {:?})",
        //     std::any::type_name::<T>(),
        //     std::any::TypeId::of::<T>(),
        // );
        self.variables.insert(name, Box::new(value));
    }

    /// Loads a variable of the given type.
    pub fn load<T: Any + Clone>(&self, name: &str) -> Result<T, Error> {
        self.variables.get(name).map_or_else(
            || {
                self.parent.as_ref().map_or_else(
                    || Err(Error::NoSuchVariable(name.into())),
                    |parent| parent.load(name),
                )
            },
            |b| {
                b.downcast_ref::<T>().cloned().ok_or_else(|| {
                    Error::IncorrectVariableType {
                        name: name.into(),
                        expected_type: std::any::type_name::<T>().into(),
                    }
                })
            },
        )
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
    (with $name:ident, $builder:expr) => {
        #[cfg(feature = "builder")]
        inventory::submit! {
            $crate::builder::WrapperRecipe {
                name: stringify!($name),
                builder: |config, context| {
                    let builder: fn(&::serde_json::Value, &$crate::builder::Context) -> Result<_, $crate::builder::Error> = $builder;
                    let wrapper = (builder)(config, context)?;

                    Ok(Box::new(move |view| {
                        let view = (wrapper)(view);
                        $crate::views::BoxedView::boxed(view)
                    }))
                }
            }
        }
    };
    ($name:ident, $builder:expr) => {
        #[cfg(feature = "builder")]
        inventory::submit! {
            $crate::builder::Recipe {
                name: stringify!($name),
                builder: |config, context| {
                    let builder: fn(&::serde_json::Value, &$crate::builder::Context) -> Result<_,$crate::builder::Error> = $builder;
                    (builder)(config, context).map($crate::views::BoxedView::boxed)
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_load_config() {
        use crate::view::Finder;

        let config = r#"
            LinearLayout:
                children:
                    - TextView:
                        content: $foo
                        with:
                            - name: text
                    - DummyView
                    - TextView: bar
                    - LinearLayout:
                        orientation: horizontal
                        children:
                            - TextView: "Age?"
                            - DummyView
                            - EditView:
                                with:
                                    - name: edit
                with:
                    - full_screen
        "#;

        let foo = "Foo";

        let config: crate::builder::Config =
            serde_yaml::from_str(config).unwrap();

        let mut context = crate::builder::Context::new();

        // Here we're still missing the $foo variable.
        assert!(context.validate(&config).is_err());

        context.store("foo", foo.to_string());

        // Now everything is find.
        context.validate(&config).unwrap();

        // Build the view from the config
        let mut res = context.build(&config).unwrap();

        // The top-level view should be a full-screen view
        assert!(res.downcast_ref::<crate::views::ResizedView<crate::views::BoxedView>>().is_some());

        // The view should be reachable by name
        let content = res
            .call_on_name("text", |v: &mut crate::views::TextView| {
                v.get_content()
            })
            .unwrap();

        assert_eq!(content.source(), foo);
    }
}
