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
pub type BareBuilder =
    fn(&serde_json::Value, &Context) -> Result<BoxedView, Error>;

/// Boxed builder
type BoxedBuilder =
    Box<dyn Fn(&serde_json::Value, &Context) -> Result<BoxedView, Error>>;

/// Can build a wrapper from a config.
pub type BareWrapperBuilder =
    fn(&serde_json::Value, &Context) -> Result<Wrapper, Error>;

/// Boxed wrapper builder
type BoxedWrapperBuilder =
    Box<dyn Fn(&serde_json::Value, &Context) -> Result<Wrapper, Error>>;

/// Can wrap a view.
pub type Wrapper = Box<dyn FnOnce(BoxedView) -> BoxedView>;

/// Can build a callback
pub type BareVarBuilder =
    fn(&serde_json::Value, &Context) -> Result<Box<dyn Any>, Error>;

/// Boxed variable builder
///
/// If you store a variable of this type, when loading type `T`, it will run
/// this builder and try to downcast the result to `T`.
pub type BoxedVarBuilder =
    Rc<dyn Fn(&serde_json::Value, &Context) -> Result<Box<dyn Any>, Error>>;

/// Everything needed to prepare a view from a config.
/// - Current recipes
/// - Any stored variables/callbacks
pub struct Context {
    variables: Rc<Variables>,
    recipes: Rc<Recipes>,
}

struct Recipes {
    recipes: HashMap<String, BoxedBuilder>,
    wrappers: HashMap<String, BoxedWrapperBuilder>,
    parent: Option<Rc<Recipes>>,
}

impl Recipes {
    fn build(
        &self,
        name: &str,
        config: &Config,
        context: &Context,
    ) -> Result<BoxedView, Error> {
        if let Some(recipe) = self.recipes.get(name) {
            (recipe)(config, context)
                .map_err(|e| Error::RecipeFailed(name.into(), Box::new(e)))
        } else {
            match self.parent {
                Some(ref parent) => parent.build(name, config, context),
                None => Err(Error::RecipeNotFound(name.into())),
            }
        }
    }

    fn build_wrapper(
        &self,
        name: &str,
        config: &Config,
        context: &Context,
    ) -> Result<Wrapper, Error> {
        if let Some(recipe) = self.wrappers.get(name) {
            (recipe)(config, context)
                .map_err(|e| Error::RecipeFailed(name.into(), Box::new(e)))
        } else {
            match self.parent {
                Some(ref parent) => {
                    parent.build_wrapper(name, config, context)
                }
                None => Err(Error::RecipeNotFound(name.into())),
            }
        }
    }
}

/// Stored variable that indicates we need to load _another_ variable.
struct VarProxy(String);

struct Variables {
    variables: HashMap<String, Box<dyn Any>>,
    parent: Option<Rc<Variables>>,
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

    #[doc(hidden)]
    _Internal,
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

impl FromConfig for crate::align::HAlign {
    fn from_config(config: &Config) -> Option<Self> {
        Some(match config.as_str()? {
            "Left" | "left" => Self::Left,
            "Center" | "center" => Self::Center,
            "Right" | "right" => Self::Right,
            _ => return None,
        })
    }
}

new_default!(Context);

impl Context {
    /// Prepare a new context using registered recipes.
    pub fn new() -> Self {
        // Collect a distributed set of recipes.
        let recipes = inventory::iter::<Recipe>()
            .map(|recipe| recipe.as_tuple())
            .collect();

        let wrappers = inventory::iter::<WrapperRecipe>()
            .map(|recipe| recipe.as_tuple())
            .collect();

        // Store callback recipes as variables for now.
        let variables = inventory::iter::<CallbackRecipe>()
            .map(|recipe| recipe.as_tuple())
            .collect();

        let recipes = Rc::new(Recipes {
            recipes,
            wrappers,
            parent: None,
        });

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
        // Use same strategy as for recipes: always include a "config", potentially null
        if let Some(name) = parse_var(config) {
            // Option 1: a simple variable name.
            self.load_exact(name, &Config::Null)
        } else if let Some(config) = config.as_object() {
            // Option 2: an object with a key (variable name pointing to a cb
            // recipe) and a body (config for the recipe).
            let (key, value) =
                config.iter().next().ok_or_else(|| Error::InvalidConfig {
                    message: "Expected non-empty body".into(),
                    config: config.clone().into(),
                })?;

            let key =
                key.strip_prefix('$').ok_or_else(|| Error::InvalidConfig {
                    message: "Expected variable as key".into(),
                    config: config.clone().into(),
                })?;

            self.load_exact(key, value)
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
        // eprintln!("Storing {name} with {:?}", std::any::TypeId::of::<T>());
        if let Some(variables) = Rc::get_mut(&mut self.variables) {
            variables.store(name, value);
        }
    }

    /// Register a new recipe _for this context only_.
    pub fn register_recipe<F>(&mut self, name: impl Into<String>, recipe: F)
    where
        F: Fn(&Config, &Context) -> Result<BoxedView, Error> + 'static,
    {
        if let Some(recipes) = Rc::get_mut(&mut self.recipes) {
            recipes.recipes.insert(name.into(), Box::new(recipe));
        }
    }

    /// Register a new wrapper recipe _for this context only_.
    pub fn register_wrapper_recipe<F>(
        &mut self,
        name: impl Into<String>,
        recipe: F,
    ) where
        F: Fn(&Config, &Context) -> Result<Wrapper, Error> + 'static,
    {
        if let Some(recipes) = Rc::get_mut(&mut self.recipes) {
            recipes.wrappers.insert(name.into(), Box::new(recipe));
        }
    }

    /// Register a new variable recipe _for this context only_.
    pub fn register_variable_recipe<F>(
        &mut self,
        name: impl Into<String>,
        recipe: F,
    ) where
        F: Fn(&Config, &Context) -> Result<Box<dyn Any>, Error> + 'static,
    {
        if let Some(variables) = Rc::get_mut(&mut self.variables) {
            let recipe: BoxedVarBuilder = Rc::new(recipe);
            variables.variables.insert(name.into(), Box::new(recipe));
        }
    }

    /// Loads a variable of the given type.
    ///
    /// This does not require FromConfig on `T`, but will also not try to deserialize.
    pub fn load_exact<T: Any + Clone>(
        &self,
        name: &str,
        config: &Config,
    ) -> Result<T, Error> {
        self.variables.call_on_any(name, |any| {
            if let Some(var) = any.downcast_ref::<T>().cloned() {
                DescentGuidance::BubbleUp(Ok(var))
            } else if let Some(VarProxy(var)) = any.downcast_ref() {
                DescentGuidance::GoDown(Some(var.clone()))
            } else if let Some(builder) = any.downcast_ref::<BoxedVarBuilder>()
            {
                DescentGuidance::BubbleUp((builder)(config, self).and_then(
                    |result| {
                        result.downcast_ref::<T>().cloned().ok_or_else(|| {
                            Error::IncorrectVariableType {
                                name: name.into(),
                                expected_type: std::any::type_name::<T>()
                                    .into(),
                            }
                        })
                    },
                ))
            } else {
                DescentGuidance::BubbleUp(Err(Error::IncorrectVariableType {
                    name: name.into(),
                    expected_type: std::any::type_name::<T>().into(),
                }))
            }
        })
    }

    /// Loads a variable of the given type.
    pub fn load<T: FromConfig + Any + Clone>(
        &self,
        name: &str,
    ) -> Result<T, Error> {
        self.variables.call_on_any(name, |any| {
            // eprintln!("Found something! {:?}", any.type_id());
            if let Some(var) = any.downcast_ref::<T>().cloned() {
                DescentGuidance::BubbleUp(Ok(var))
            } else if let Some(config) = any.downcast_ref::<Config>() {
                DescentGuidance::BubbleUp(T::from_config(config).ok_or_else(
                    || Error::CouldNotLoad {
                        expected_type: std::any::type_name::<T>().into(),
                        config: config.clone(),
                    },
                ))
            } else {
                DescentGuidance::BubbleUp(Err(Error::IncorrectVariableType {
                    name: name.into(),
                    expected_type: std::any::type_name::<T>().into(),
                }))
            }
        })
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

        let wrapper = self.recipes.build_wrapper(key, value, self)?;

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

        let mut view = self.recipes.build(key, value, self)?;

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

        let recipes = Rc::new(Recipes {
            recipes: HashMap::new(),
            wrappers: HashMap::new(),
            parent: Some(Rc::clone(&self.recipes)),
        });

        let mut context = Context { recipes, variables };
        f(&mut context);
        context
    }

    /// Builds a view from a template config.
    ///
    /// `template` should be a config describing a view, potentially using variables.
    /// Any value in `config` will be stored as a variable when rendering the template.
    pub fn build_template(
        &self,
        config: &Config,
        template: &Config,
    ) -> Result<BoxedView, Error> {
        let res = self
            .sub_context(|c| {
                if let Some(config) = config.as_object() {
                    for (key, value) in config.iter() {
                        // If value is a variable, resolve it first.
                        if let Some(var) = parse_var(value) {
                            c.store(key, VarProxy(var.into()));
                        } else {
                            c.store(key, value.clone());
                        }
                    }
                } else {
                    c.store(".", config.clone());
                }
            })
            .build(template)?;

        Ok(res)
    }
}

fn parse_var(value: &Config) -> Option<&str> {
    value.as_str().and_then(|s| s.strip_prefix('$'))
}

enum DescentGuidance<T> {
    BubbleUp(Result<T, Error>),
    GoDown(Option<String>),
}

impl Variables {
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

    fn call_on_any<F, T>(&self, name: &str, mut f: F) -> Result<T, Error>
    where
        F: FnMut(&dyn Any) -> DescentGuidance<T>,
        T: 'static,
    {
        let guidance = match self.variables.get(name) {
            None => DescentGuidance::GoDown(None),
            Some(var) => (f)(var.as_ref()),
        };

        match guidance {
            DescentGuidance::BubbleUp(res) => return res,
            DescentGuidance::GoDown(new_name) => {
                let name =
                    new_name.as_ref().map(String::as_str).unwrap_or(name);

                self.parent.as_ref().map_or_else(
                    || Err(Error::NoSuchVariable(name.into())),
                    |parent| parent.call_on_any(name, f),
                )
            }
        }
    }
}

/// Describes how to build a callback.
pub struct CallbackRecipe {
    /// Name used in config file to use this callback.
    ///
    /// The config file will include an extra $ at the beginning.
    pub name: &'static str,

    /// Function to run this recipe.
    pub builder: BareVarBuilder,
}

impl CallbackRecipe {
    fn as_tuple(&self) -> (String, Box<dyn Any>) {
        let cb: BoxedVarBuilder = Rc::new(self.builder);
        (self.name.into(), Box::new(cb))
    }
}

/// Describes how to build a view.
pub struct Recipe {
    /// Name used in config file to use this recipe.
    pub name: &'static str,

    /// Function to run this recipe.
    pub builder: BareBuilder,
}

impl Recipe {
    fn as_tuple(&self) -> (String, BoxedBuilder) {
        (self.name.into(), Box::new(self.builder))
    }
}

/// Describes how to build a view wrapper.
pub struct WrapperRecipe {
    /// Name used in config file to use this wrapper.
    pub name: &'static str,

    /// Function to run this recipe.
    pub builder: BareWrapperBuilder,
}

impl WrapperRecipe {
    fn as_tuple(&self) -> (String, BoxedWrapperBuilder) {
        (self.name.into(), Box::new(self.builder))
    }
}

inventory::collect!(Recipe);
inventory::collect!(CallbackRecipe);
inventory::collect!(WrapperRecipe);

#[macro_export]
/// Define a recipe to build this view from a config file.
macro_rules! recipe {
    ($name:ident from $config_builder:expr) => {
        #[cfg(feature = "builder")]
        $crate::submit! {
            $crate::builder::Recipe {
                name: stringify!($name),
                builder: |config, context| {
                    let template = $config_builder;
                    context.build_template(config, &template)
                },
            }
        }
    };
    (with $name:ident, $builder:expr) => {
        #[cfg(feature = "builder")]
        $crate::submit! {
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
        $crate::submit! {
            $crate::builder::Recipe {
                name: stringify!($name),
                builder: |config, context| {
                    let builder: fn(&::serde_json::Value, &$crate::builder::Context) -> Result<_,$crate::builder::Error> = $builder;
                    (builder)(config, context).map($crate::views::BoxedView::boxed)
                },
            }
        }
    };
}

#[macro_export]
/// Define a macro for a variable builder.
macro_rules! var_recipe {
    ($name: expr, $builder:expr) => {
        #[cfg(feature = "builder")]
        $crate::submit! {
            $crate::builder::CallbackRecipe {
                name: $name,
                builder: |config, context| {
                    let builder: fn(&::serde_json::Value, &$crate::builder::Context) -> Result<_, $crate::builder::Error> = $builder;
                    Ok(Box::new((builder)(config, context)?))
                },
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
