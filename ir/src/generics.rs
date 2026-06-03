impl Context {
    pub fn push_generic_scope(
        &mut self,
        generics: &Option<ast::Span<Vec<ast::Span<ast::GenericParameter>>>>,
        mod_key: &ModuleKey,
    ) -> Result<ScopeKey, Error> {
        let scope = self.generic_ctx.push();

        if let Some(generics) = generics {
            for generic in generics.deref() {
                let mut generics = Vec::new();

                for constr_path in &generic.constraints {
                    let module = self.types.modules.get_mut_unchecked(mod_key);

                    let obj_key = module
                        .symbol_map
                        .get(constr_path.path.first().as_ref().unwrap().inner.as_ref())
                        .unwrap()
                        .clone();
                    dbg!("resolve traits like a normal person");

                    let trt_key = match obj_key {
                        AnyObjectKey::Trait(k) => k,
                        k => todo!("unexpected: {k:?}"),
                    };

                    let ty = match self.objects.traits.get_unchecked(&trt_key).data {
                        TraitObj {
                            ty: InitState::Done(ty) | InitState::Progress(ty),
                        } => ty,

                        _ => {
                            return Err(Diagnostic {
                                inner: Errors::NonConstraintType(obj_key),
                                span: generic.identifier.location,
                                module: *mod_key,
                            });
                        }
                    };

                    generics.push(ty);
                }

                let constraint = self.types.constraints.push_unique(ConstraintType {
                    constraints: generics,
                });

                let ident = generic.identifier.deref();

                let generic = GenericType {
                    ident: ident.clone(),
                    constraint,
                };

                let gen_key = self.types.generics.push(generic);

                self.generic_ctx.insert(ident.clone(), gen_key);
            }
        }

        Ok(scope)
    }
}

use std::ops::Deref;

use arena_scope::{ScopeKey, ScopeTree};
use smol_str::SmolStr;

use crate::{
    ast,
    const_stage::{
        Context, Diagnostic, Error, Errors,
        objects::{AnyObjectKey, InitState, TraitObj},
        types::{ConstraintType, GenericKey, GenericType, ModuleKey},
    },
};

pub type GContext = ScopeTree<SmolStr, GenericKey>;
