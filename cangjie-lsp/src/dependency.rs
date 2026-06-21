mod resolver;
mod types;

#[cfg(test)]
mod tests;

pub use resolver::DependencyResolver;
pub use types::{Dependency, ModuleOption, PackageRequires};

#[cfg(test)]
use crate::utils::*;
