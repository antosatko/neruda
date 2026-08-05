use std::{alloc::Layout, collections::HashMap};

use cranelift::codegen::isa::TargetFrontendConfig;
use ir::const_stage::{
    Errors,
    types::{AnyTypeKey, PrimitiveType, Types, Vector},
};

#[derive(Default)]
pub struct Layouts(pub HashMap<AnyTypeKey, LayoutOptions>);

#[derive(Debug, Clone)]
pub struct CompoundFieldLayout {
    offset: usize,
}

#[derive(Debug, Clone)]
pub enum LayoutOptions {
    Unsized { align: usize },
    Primitive(Layout),
    Compound(Vec<CompoundFieldLayout>, Layout),
    Void,
}

impl Layouts {
    pub fn of(
        &mut self,
        ty: &AnyTypeKey,
        types: &Types,
        cfg: &TargetFrontendConfig,
    ) -> LayoutOptions {
        let ty = &ty.unwrap_full(types);
        let l = match self.0.get(ty) {
            Some(l) => return l.clone(),
            None => match &ty {
                AnyTypeKey::Primitive(primitive_type) => {
                    let v = size_of_prim(*primitive_type);
                    return LayoutOptions::Primitive(unsafe {
                        Layout::from_size_align_unchecked(v, v)
                    });
                }
                AnyTypeKey::Function(_) | AnyTypeKey::Reference(_) => {
                    LayoutOptions::Primitive(unsafe {
                        Layout::from_size_align_unchecked(
                            cfg.pointer_bytes() as _,
                            cfg.pointer_bytes() as _,
                        )
                    })
                }
                AnyTypeKey::Vector(Vector { element, lanes }) => {
                    let v = size_of_prim(*element);
                    let total = v * (*lanes as usize);
                    LayoutOptions::Primitive(unsafe {
                        Layout::from_size_align_unchecked(total, total)
                    })
                }
                AnyTypeKey::Array(key) => todo!(),
                AnyTypeKey::Tuple(key) => todo!(),
                AnyTypeKey::Struct(key) => todo!(),
                AnyTypeKey::Enum(key) => {
                    let repr = types.enums.get_unchecked(key).repr;
                    self.of(&repr, types, cfg)
                }
                AnyTypeKey::Void | AnyTypeKey::Never => LayoutOptions::Void,
                AnyTypeKey::Generic(_)
                | AnyTypeKey::Morphed(_)
                | AnyTypeKey::ModuleRef(_)
                | AnyTypeKey::Named(_)
                | AnyTypeKey::Polymorph(_)
                | AnyTypeKey::Trait(_) => unreachable!("hi :)"),
            },
        };
        self.0.insert(*ty, l.clone());
        l
    }

    fn struct_layout(
        &mut self,
        fields: impl IntoIterator<Item = AnyTypeKey>,
        types: &Types,
        cfg: &TargetFrontendConfig,
    ) -> Result<(Vec<CompoundFieldLayout>, Layout), Errors> {
        let mut field_layouts = Vec::new();
        let mut offset = 0;
        let mut max_align = 1;

        for field in fields {
            let layout = self.of(&field, types, cfg);

            max_align = max_align.max(layout.align().ok_or(Errors::TypeIsUnsized(field))?);

            offset = align_up(offset, layout.align().ok_or(Errors::TypeIsUnsized(field))?);

            field_layouts.push(CompoundFieldLayout { offset });

            offset += layout.size().ok_or(Errors::TypeIsUnsized(field))?;
        }

        let size = align_up(offset, max_align);

        Ok((field_layouts, unsafe {
            Layout::from_size_align_unchecked(size, max_align)
        }))
    }
}

impl LayoutOptions {
    pub fn size(&self) -> Option<usize> {
        match self {
            LayoutOptions::Primitive(layout) | LayoutOptions::Compound(_, layout) => {
                Some(layout.size())
            }
            LayoutOptions::Unsized { .. } => None,
            LayoutOptions::Void => None,
        }
    }

    pub fn align(&self) -> Option<usize> {
        match self {
            LayoutOptions::Primitive(layout) | LayoutOptions::Compound(_, layout) => {
                Some(layout.align())
            }
            LayoutOptions::Unsized { align } => Some(*align),
            LayoutOptions::Void => None,
        }
    }

    pub fn layout(&self) -> Option<&Layout> {
        match self {
            LayoutOptions::Primitive(layout) | LayoutOptions::Compound(_, layout) => Some(layout),
            LayoutOptions::Unsized { .. } => None,
            LayoutOptions::Void => None,
        }
    }
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());

    (value + align - 1) & !(align - 1)
}

fn size_of_prim(prim: PrimitiveType) -> usize {
    match prim {
        ir::const_stage::types::PrimitiveType::I8
        | ir::const_stage::types::PrimitiveType::U8
        | ir::const_stage::types::PrimitiveType::Bool => 1,
        ir::const_stage::types::PrimitiveType::I16 | ir::const_stage::types::PrimitiveType::U16 => {
            2
        }
        ir::const_stage::types::PrimitiveType::I32
        | ir::const_stage::types::PrimitiveType::U32
        | ir::const_stage::types::PrimitiveType::F32
        | ir::const_stage::types::PrimitiveType::Char => 4,
        ir::const_stage::types::PrimitiveType::I64
        | ir::const_stage::types::PrimitiveType::U64
        | ir::const_stage::types::PrimitiveType::F64 => 8,
        ir::const_stage::types::PrimitiveType::I128
        | ir::const_stage::types::PrimitiveType::U128 => 16,
        ir::const_stage::types::PrimitiveType::EntityRef => todo!(),
    }
}
