//! Extended clap: namespaced config types that work with serde and clap, without a `clap`
//! dependency.
//!
//! `#[derive(eclap::Parser)]` makes a root type with inherent `parse`, `try_parse`,
//! `parse_from` and `try_parse_from`. `#[derive(eclap::Args)]` makes a reusable config
//! type (structs, and enums selected by a `type` flag) that carries no naming of its own:
//! the field that holds it decides the flag names.
//!
//! ```
//! #[derive(eclap::Parser)]
//! struct Cli {
//!     #[eclap(ns = "db")]
//!     db: Db,
//!     #[arg(short, env)]
//!     verbose: bool,
//! }
//!
//! #[derive(eclap::Args)]
//! struct Db {
//!     host: String,
//!     #[eclap(nested)]
//!     auth: Auth,
//! }
//!
//! #[derive(eclap::Args)]
//! enum Auth {
//!     Anonymous,
//!     Password(Password),
//! }
//!
//! #[derive(eclap::Args)]
//! struct Password {
//!     user: String,
//! }
//!
//! let cli = Cli::try_parse_from([
//!     "bin", "-v", "--db.host", "h", "--db.auth.type", "Password", "--db.auth.password.user", "u",
//! ])
//! .unwrap();
//! assert!(cli.verbose);
//! assert_eq!(cli.db.host, "h");
//! assert!(matches!(cli.db.auth, Auth::Password(Password { user }) if user == "u"));
//! ```
//!
//! # Flag names
//!
//! A flag is the path of segments joined by a separator, then the field name; words inside
//! one segment stay kebab-case. `#[eclap(nested)]` adds the field name as a segment,
//! `#[eclap(ns = "x")]` adds `x` (`""` adds none), and
//! `#[eclap(ns_sep = '-')]` sets the separator after the segment and below it
//! (default `.`). An enum's flags live in its field's path: the tag is `<path>type` and always
//! reads its env var, and a
//! variant's fields are `<path><kebab-variant><sep><field>`, required only when the tag
//! (and every outer tag) selects that variant. `#[arg(..)]` on a value field forwards to
//! the clap `Arg` (`key = expr` or `key(args)`), plus bare `short`, `env` (`A.B-c` becomes
//! `A__B_C`) and `default_value_t`. `#[command(..)]` does the same on a Parser type.
//! `#[eclap(flatten)]` and `#[eclap(subcommand)]` take plain clap `Args` / `Subcommand` types.
//!
//! clap's own derives (`Subcommand`, `ValueEnum`) work without a clap dependency when the
//! module has `use eclap::clap;`.
//!
//! Two fields that map to the same flag panic when the command is built, in every build.
//!
//! `update_from_arg_matches` / `try_update_from` re-parse the whole value: every required
//! flag must be given again and optional flags that are not given are reset.

use std::any::Any;

pub use clap;
pub use eclap_derive::{Args, Parser};

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command, Error};

/// A type whose fields can be registered on a [`Command`] and read back from [`ArgMatches`].
pub trait Args: Sized {
    fn augment(cmd: Command, scope: &Scope) -> Command;
    fn from_matches(matches: &ArgMatches, scope: &Scope) -> Result<Self, Error>;
}

/// Flag-name prefix, separator and "required only when" conditions of the type being registered.
#[derive(Clone, Debug)]
pub struct Scope {
    /// Path of segments, each followed by its separator.
    prefix: String,
    sep: char,
    when: Vec<(String, &'static str)>,
}

impl Scope {
    #[doc(hidden)]
    pub fn root() -> Scope {
        Scope {
            prefix: String::new(),
            sep: '.',
            when: Vec::new(),
        }
    }

    /// Argument id, which is also its long flag.
    #[doc(hidden)]
    pub fn id(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }

    /// Environment variable of the argument `name`.
    #[doc(hidden)]
    pub fn env(&self, name: &str) -> String {
        self.id(name)
            .to_uppercase()
            .replace('.', "__")
            .replace('-', "_")
    }

    /// Scope of a field's type: `segment` joins the path with this scope's separator and
    /// `sep` (if any) replaces the separator after it and below it.
    #[doc(hidden)]
    pub fn child(&self, segment: &str, sep: Option<char>) -> Scope {
        let sep = sep.unwrap_or(self.sep);
        let mut prefix = self.prefix.clone();
        if !segment.is_empty() {
            prefix.push_str(segment);
            prefix.push(sep);
        }
        Scope {
            prefix,
            sep,
            when: self.when.clone(),
        }
    }

    /// Scope of one enum variant: its fields are required only when the tag argument
    /// `tag_id` equals `value`.
    #[doc(hidden)]
    pub fn variant(&self, tag_id: &str, value: &'static str, segment: &str) -> Scope {
        let mut when = self.when.clone();
        when.push((tag_id.to_owned(), value));
        Scope {
            prefix: format!("{}{segment}{}", self.prefix, self.sep),
            sep: self.sep,
            when,
        }
    }

    #[doc(hidden)]
    pub fn arg(
        &self,
        name: &str,
        help: Option<&'static str>,
        long_help: Option<&'static str>,
    ) -> Arg {
        let id = self.id(name);
        Arg::new(id.clone())
            .long(id)
            .value_name(name.to_uppercase().replace('-', "_"))
            .help(help)
            .long_help(long_help)
    }

    /// Panics in release builds too: clap only checks duplicate flags in debug builds.
    #[doc(hidden)]
    pub fn add(&self, cmd: Command, arg: Arg) -> Command {
        let id = arg.get_id().as_str();
        let clash = cmd.get_arguments().find_map(|a| {
            if a.get_id().as_str() == id {
                Some(format!("--{id}"))
            } else if a.get_long().is_some() && a.get_long() == arg.get_long() {
                Some(format!("--{}", a.get_long()?))
            } else if a.get_short().is_some() && a.get_short() == arg.get_short() {
                Some(format!("-{}", a.get_short()?))
            } else {
                None
            }
        });
        assert!(
            clash.is_none(),
            "eclap: duplicate flag `{}` (adding `--{id}`); two fields map to the same flag (check `ns = \"\"`, `long` and `short`)",
            clash.unwrap_or_default()
        );
        cmd.arg(arg)
    }

    /// Panics in all builds: clap cannot make flattened args or subcommands conditional.
    #[doc(hidden)]
    pub fn unconditional(&self, field: &str) {
        assert!(
            self.when.is_empty(),
            "eclap: `flatten`/`subcommand` fields are not supported inside enum variants (field `{field}`)"
        );
    }

    /// Requires `arg` unless the user's forwarded calls gave it another action or a default.
    #[doc(hidden)]
    pub fn required_if_needed(&self, arg: Arg) -> Arg {
        if matches!(arg.get_action(), ArgAction::Set) && arg.get_default_values().is_empty() {
            self.required(arg)
        } else {
            arg
        }
    }

    #[doc(hidden)]
    pub fn required(&self, arg: Arg) -> Arg {
        match self.when.as_slice() {
            [] => arg.required(true),
            [(id, value)] => arg.required_if_eq(id.clone(), *value),
            all => arg.required_if_eq_all(all.iter().map(|(id, value)| (id.clone(), *value))),
        }
    }

    #[doc(hidden)]
    pub fn one<T: Any + Clone + Send + Sync>(
        &self,
        matches: &ArgMatches,
        name: &str,
    ) -> Result<Option<T>, Error> {
        matches
            .try_get_one::<T>(&self.id(name))
            .map(|value| value.cloned())
            .map_err(|e| Error::raw(ErrorKind::InvalidValue, e))
    }

    #[doc(hidden)]
    pub fn require<T: Any + Clone + Send + Sync>(
        &self,
        matches: &ArgMatches,
        name: &str,
    ) -> Result<T, Error> {
        self.one(matches, name)?.ok_or_else(|| {
            Error::raw(
                ErrorKind::MissingRequiredArgument,
                format!(
                    "the following required argument was not provided: --{}\n",
                    self.id(name)
                ),
            )
        })
    }

    #[doc(hidden)]
    pub fn many<T: Any + Clone + Send + Sync>(
        &self,
        matches: &ArgMatches,
        name: &str,
    ) -> Result<Vec<T>, Error> {
        matches
            .try_get_many::<T>(&self.id(name))
            .map(|values| values.map(|v| v.cloned().collect()).unwrap_or_default())
            .map_err(|e| Error::raw(ErrorKind::InvalidValue, e))
    }

    #[doc(hidden)]
    pub fn flag(&self, matches: &ArgMatches, name: &str) -> Result<bool, Error> {
        Ok(self.one::<bool>(matches, name)?.unwrap_or(false))
    }

    /// Registers the fields of variant type `T` under the help heading `--<tag_id> <value>`.
    #[doc(hidden)]
    pub fn augment_variant<T: Args>(
        &self,
        cmd: Command,
        tag_id: &str,
        value: &'static str,
        segment: &str,
    ) -> Command {
        let previous = cmd.get_next_help_heading().map(str::to_owned);
        let cmd = cmd.next_help_heading(format!("--{tag_id} {value}"));
        let cmd = T::augment(cmd, &self.variant(tag_id, value, segment));
        // Parent args declared after this one must not land under the variant heading.
        match previous {
            Some(heading) => cmd.next_help_heading(heading),
            None => cmd.next_help_heading(None::<&'static str>),
        }
    }
}
