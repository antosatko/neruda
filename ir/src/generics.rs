impl Context {
    pub fn push_generic_scope(
        &mut self,
        generics: &Option<ast::Span<Vec<ast::Span<ast::GenericParameter>>>>,
        mod_key: &ModuleKey,
    ) -> Result<ScopeKey, Error> {
        let scope = self.generic_ctx.push();

        if let Some(generics) = generics {
            for generic in generics.deref() {
                let mut constraints = Vec::new();

                for constr_path in &generic.constraints {
                    let module = self.types.modules.get_mut_unchecked(mod_key);

                    let obj_key = module
                        .symbol_map
                        .get(constr_path.path.first().as_ref().unwrap().inner.as_ref())
                        .unwrap()
                        .clone();

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

                    constraints.push(ty);
                }

                let constraint = if constraints.is_empty() {
                    self.auto_types.any_conr
                } else {
                    self.types.constraints.push(ConstraintType { constraints })
                };

                self.generic_ctx
                    .insert(generic.identifier.inner.as_ref().clone(), constraint);
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
        types::{ConstraintKey, ConstraintType, ModuleKey},
    },
};

pub type GContext = ScopeTree<SmolStr, ConstraintKey>;
