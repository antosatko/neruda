use std::{collections::HashMap, ops::Deref};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::{
    ast,
    const_stage::{
        Context, Diagnostic, Error, Errors,
        objects::{AnyObjectKey, InitState, TraitObj},
        types::{ConstraintKey, ConstraintType, ModuleKey},
    },
};

pub type GScopeKey = Key<GScopeTag>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GScopeTag;

#[derive(Debug, Default, Clone)]
pub struct GScope {
    pub symbols: Vec<(SmolStr, ConstraintKey)>,
}

impl GScope {
    #[inline]
    pub fn get(&self, ident: &str) -> Option<ConstraintKey> {
        self.symbols
            .iter()
            .find_map(|(i, v)| (i == ident).then(|| *v))
    }

    #[inline]
    pub fn set(&mut self, ident: SmolStr, constraint: ConstraintKey) {
        self.symbols.push((ident, constraint));
    }
}

#[derive(Debug, Clone)]
pub struct GScopeNode {
    pub parent: Option<GScopeKey>,
    pub scope: GScope,
}

impl GScopeNode {
    #[inline]
    pub fn new(parent: Option<GScopeKey>) -> Self {
        Self {
            parent,
            scope: GScope::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct GContext {
    pub data: Arena<GScopeNode, GScopeTag>,
    pub current: Option<GScopeKey>,
}

impl GContext {
    #[inline]
    pub fn init(&mut self) {
        debug_assert!(self.current.is_none());

        self.current = Some(self.data.push(GScopeNode::new(None)));
    }

    #[inline]
    pub fn destroy(&mut self) {
        debug_assert!(self.current.is_some());

        self.current = None;
    }

    #[inline]
    pub fn current(&self) -> GScopeKey {
        debug_assert!(self.current.is_some());

        unsafe { self.current.unwrap_unchecked() }
    }

    #[inline]
    pub fn current_node(&self) -> &GScopeNode {
        let key = self.current();

        self.data.get_unchecked(&key)
    }

    #[inline]
    pub fn current_node_mut(&mut self) -> &mut GScopeNode {
        let key = self.current();

        self.data.get_mut_unchecked(&key)
    }

    /// Pushes a new child scope.
    pub fn push(&mut self) -> GScopeKey {
        let parent = self.current;

        let key = self.data.push(GScopeNode::new(parent));

        self.current = Some(key);

        key
    }

    /// Pops the current scope and returns the popped scope key.
    pub fn pop(&mut self) -> GScopeKey {
        let current = self.current();

        let parent = self.current_node().parent;

        debug_assert!(parent.is_some(), "attempted to pop root generic scope");

        self.current = parent;

        current
    }

    #[inline]
    pub fn insert(&mut self, ident: SmolStr, constraint: ConstraintKey) {
        self.current_node_mut().scope.set(ident, constraint);
    }

    pub fn get(&self, ident: &str) -> Option<ConstraintKey> {
        let mut current = self.current;

        while let Some(key) = current {
            let node = self.data.get_unchecked(&key);

            if let Some(found) = node.scope.get(ident) {
                return Some(found);
            }

            current = node.parent;
        }

        None
    }

    #[inline]
    pub fn snapshot(&self) -> Option<GScopeKey> {
        self.current
    }

    #[inline]
    pub fn restore(&mut self, key: GScopeKey) {
        self.current = Some(key);
    }
}

impl Context {
    pub fn push_generic_scope(
        &mut self,
        generics: &Option<ast::Span<Vec<ast::Span<ast::GenericParameter>>>>,
        mod_key: &ModuleKey,
    ) -> Result<GScopeKey, Error> {
        let scope = self.generic_ctx.push();

        if let Some(generics) = generics {
            for generic in generics.inner.deref() {
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
