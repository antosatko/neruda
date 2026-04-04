use crate::ast::Module;

pub struct FormatOptions {}

impl Module {
    fn fmt(&self, mut f: impl std::fmt::Write, opt: &FormatOptions) -> std::fmt::Result {
        for docs in &self.docs {
            writeln!(f, "//! {}", docs.inner)?;
        }
        Ok(())
    }
}
