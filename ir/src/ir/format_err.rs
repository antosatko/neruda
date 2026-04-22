use std::{fmt::Write, path::Path};

use crate::ir::{Context, Error, Errors};

impl Error {
    pub fn write(
        &self,
        w: &mut impl Write,
        _txt: &str,
        filepath: Option<&Path>,
        ctx: &Context,
    ) -> std::fmt::Result {
        let (id, header) = self.inner.id_header(ctx);
        write!(w, "ERR[{id}]")?;
        match filepath {
            Some(path) => write!(w, "{:?}:{}: ", self.span, path.to_string_lossy()),
            None => write!(w, "{:?}: ", self.span),
        }?;
        write!(w, "{header}")
    }

    pub fn print(&self, txt: &str, filepath: Option<&Path>, ctx: &Context) -> std::fmt::Result {
        let mut msg = String::new();
        self.write(&mut msg, txt, filepath, ctx)?;
        println!("{msg}");
        Ok(())
    }
}

impl Errors {
    pub fn id_header(&self, ctx: &Context) -> (&'static str, String) {
        match self {
            Self::IllegalType(ty, ty_mod) => {
                let module = ctx.types.modules.get_unchecked(&ty_mod);
                let obj = module.objects.get_unchecked(ty);
                let ident = &obj.identifier;
                ("400", format!("Use of type {ident} is illegal here"))
            }
            Self::NonConstraintType(obj, obj_module) => {
                let module = ctx.types.modules.get_unchecked(&obj_module);
                let obj = module.objects.get_unchecked(&obj);
                let ident = &obj.identifier;
                ("401", format!("Type {ident} must be a trait type"))
            }
            Self::CouldNotSubstituteType(ty) => (
                "402",
                format!("Could not substitute for type {}", ty.stringify(&ctx.types)),
            ),
            _ => todo!(),
        }
    }
}
