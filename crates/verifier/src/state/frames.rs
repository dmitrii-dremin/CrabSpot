use super::{
    symbols::{Proof, Ty},
    *,
};
use crabspot_classfile::{Code, StackMapFrameKind as K, VerificationType as V};

#[derive(Debug)]
pub(super) struct Prefix {
    pub values: Vec<Ty>,
    pub this_count: usize,
}
#[derive(Debug)]
pub(super) struct Map {
    pub offset: u32,
    pub locals: usize,
    pub stack: Vec<Ty>,
    pub pending: bool,
    pub location: CodeValidationLocation,
}
#[derive(Debug)]
pub(super) enum Locals {
    Shared(usize),
    Owned(Vec<Ty>),
}
#[derive(Debug)]
pub(super) struct Frame {
    pub locals: Locals,
    pub stack: Vec<Ty>,
    pub pending: bool,
}
impl<'e, 'a> Proof<'e, 'a> {
    pub fn local_values<'b>(&'b self, locals: &'b Locals) -> T<&'b [Ty]> {
        match locals {
            Locals::Shared(id) => self
                .arena
                .get(*id)
                .map(|p| p.values.as_slice())
                .ok_or(bad("invalid local arena index")),
            Locals::Owned(v) => Ok(v),
        }
    }
    fn arena_values(&self, id: usize) -> T<&[Ty]> {
        self.arena
            .get(id)
            .map(|p| p.values.as_slice())
            .ok_or(bad("invalid map local arena index"))
    }
    pub fn arena_copy(&mut self, id: usize, cut: usize, extra: &[Ty]) -> T<usize> {
        let old = self.arena_values(id)?;
        if cut > old.len() {
            return Err(bad("invalid local prefix copy"));
        }
        let n = cut
            .checked_add(extra.len())
            .ok_or(bad("local prefix size overflow"))?;
        self.cells(n)?;
        let mut values = Vec::new();
        reserve(&mut values, n, "local prefixes")?;
        values.extend_from_slice(
            self.arena_values(id)?
                .get(..cut)
                .ok_or(bad("invalid local prefix slice"))?,
        );
        values.extend_from_slice(extra);
        self.publish_prefix(values)
    }
    pub fn publish_prefix(&mut self, values: Vec<Ty>) -> T<usize> {
        self.work(values.len())?;
        let this_count = values.iter().filter(|v| **v == Ty::This).count();
        reserve(&mut self.arena, 1, "local arena")?;
        let id = self.arena.len();
        self.arena.push(Prefix { values, this_count });
        Ok(id)
    }
    pub fn local_owned(&mut self, frame: &mut Frame) -> T<()> {
        if let Locals::Shared(id) = frame.locals {
            let n = self.arena_values(id)?.len();
            self.cells(n)?;
            let mut copy = Vec::new();
            reserve(&mut copy, n, "current locals")?;
            copy.extend_from_slice(self.arena_values(id)?);
            frame.locals = Locals::Owned(copy);
        }
        Ok(())
    }
    pub fn push(&mut self, frame: &mut Frame, ty: Ty, max_stack: u16) -> T<()> {
        if matches!(ty, Ty::Top | Ty::WideTail) {
            return Err(bad("cannot push an unusable value"));
        }
        let width = ty.width();
        if frame
            .stack
            .len()
            .checked_add(width)
            .is_none_or(|n| n > usize::from(max_stack))
        {
            return Err(bad("operand stack exceeds max_stack"));
        }
        self.cells(width)?;
        reserve(&mut frame.stack, width, "current stack")?;
        frame.stack.push(ty);
        if width == 2 {
            frame.stack.push(Ty::WideTail);
        }
        Ok(())
    }
    pub fn pop(&mut self, frame: &mut Frame) -> T<Ty> {
        if frame.stack.is_empty() {
            return Err(bad("operand stack underflow"));
        }
        self.work(1)?;
        let last = *frame.stack.last().ok_or(bad("operand stack underflow"))?;
        if last == Ty::WideTail {
            self.work(1)?;
            let i = frame
                .stack
                .len()
                .checked_sub(2)
                .ok_or(bad("orphan wide stack tail"))?;
            let head = *frame.stack.get(i).ok_or(bad("orphan wide stack tail"))?;
            if head.width() != 2 {
                return Err(bad("orphan wide stack tail"));
            }
            frame.stack.truncate(i);
            Ok(head)
        } else {
            if matches!(last, Ty::Top | Ty::Long | Ty::Double) {
                return Err(bad("invalid stack value or missing wide tail"));
            }
            frame.stack.pop();
            Ok(last)
        }
    }
    pub fn pop_exact(&mut self, frame: &mut Frame, expected: Ty) -> T<Ty> {
        let ty = self.pop(frame)?;
        if ty == expected {
            Ok(ty)
        } else {
            Err(bad("wrong operand computational type"))
        }
    }
    pub fn pop_reference(&mut self, frame: &mut Frame, ordinary: bool) -> T<Ty> {
        let ty = self.pop(frame)?;
        if if ordinary {
            ty.ordinary()
        } else {
            ty.generic()
        } {
            Ok(ty)
        } else {
            Err(bad("wrong reference operand type"))
        }
    }
    pub fn load(&mut self, frame: &Frame, index: usize, expected: Option<Ty>) -> T<Ty> {
        self.work(1)?;
        let values = self.local_values(&frame.locals)?;
        let ty = values.get(index).copied().unwrap_or(Ty::Top);
        if let Some(expected) = expected {
            if ty != expected {
                return Err(bad("wrong local computational type"));
            }
        } else if !ty.generic() {
            return Err(bad("aload requires a reference local"));
        }
        if ty.width() == 2 {
            self.work(1)?;
            if self.local_values(&frame.locals)?.get(index + 1) != Some(&Ty::WideTail) {
                return Err(bad("wide local lacks continuation"));
            }
        }
        Ok(ty)
    }
    pub fn store(&mut self, frame: &mut Frame, index: usize, value: Ty, max_locals: u16) -> T<()> {
        let end = index
            .checked_add(value.width())
            .ok_or(bad("local store index overflow"))?;
        if end > usize::from(max_locals) {
            return Err(bad("local store exceeds max_locals"));
        }
        self.local_owned(frame)?;
        let Locals::Owned(values) = &mut frame.locals else {
            return Err(bad("COW local ownership failed"));
        };
        if end > values.len() {
            let n = end - values.len();
            self.cells(n)?;
            reserve(values, n, "current local extension")?;
            values.resize(end, Ty::Top);
        }
        // The overwritten range plus at most one neighboring category-two head
        // and tail are examined before any change; absent suffix remains Top.
        let start = index.saturating_sub(1);
        let stop = end.saturating_add(1).min(values.len());
        self.work(stop - start)?;
        if values.get(index) == Some(&Ty::WideTail) && index > 0 {
            if let Some(v) = values.get_mut(index - 1) {
                *v = Ty::Top;
            }
        }
        for i in index..end {
            if values.get(i).is_some_and(|v| v.width() == 2) {
                if let Some(v) = values.get_mut(i + 1) {
                    *v = Ty::Top;
                }
            }
        }
        *values
            .get_mut(index)
            .ok_or(bad("local store disappeared"))? = value;
        if value.width() == 2 {
            *values
                .get_mut(index + 1)
                .ok_or(bad("wide store continuation disappeared"))? = Ty::WideTail;
        }
        Ok(())
    }
    fn verification(&mut self, value: &V) -> T<Ty> {
        Ok(match value {
            V::Top => Ty::Top,
            V::Integer => Ty::Int,
            V::Float => Ty::Float,
            V::Long => Ty::Long,
            V::Double => Ty::Double,
            V::Null => Ty::Null,
            V::UninitializedThis => Ty::This,
            V::Uninitialized(offset) => Ty::Uninit(u32::from(*offset)),
            V::Object(index) => self.class_ty(self.class, *index)?,
        })
    }
    fn expand(&mut self, types: &[V]) -> T<Vec<Ty>> {
        self.work(types.len())?;
        let mut total = 0usize;
        for ty in types {
            total = total
                .checked_add(if matches!(ty, V::Long | V::Double) {
                    2
                } else {
                    1
                })
                .ok_or(bad("map size overflow"))?;
        }
        self.cells(total)?;
        let mut values = Vec::new();
        reserve(&mut values, total, "expanded map cells")?;
        for value in types {
            let ty = self.verification(value)?;
            values.push(ty);
            if ty.width() == 2 {
                values.push(Ty::WideTail);
            }
        }
        Ok(values)
    }
    pub fn prepare_maps(&mut self, code: &Code, initial: usize) -> T<()> {
        for (attribute, a) in code.attributes.iter().enumerate() {
            self.work(1)?;
            let crabspot_classfile::AttributeBody::StackMapTable(frames) = &a.body else {
                continue;
            };
            let mut last = initial;
            for (frame_index, frame) in frames.iter().enumerate() {
                self.location = CodeValidationLocation::StackMapFrame {
                    attribute,
                    frame: frame_index,
                };
                self.work(1)?;
                let mut stack = Vec::new();
                let locals = match &frame.kind {
                    K::Same => last,
                    K::SameLocalsOneStack(v) => {
                        stack = self.expand(std::slice::from_ref(v))?;
                        last
                    }
                    K::Full {
                        locals,
                        stack: values,
                    } => {
                        let locals = self.expand(locals)?;
                        stack = self.expand(values)?;
                        self.publish_prefix(locals)?
                    }
                    K::Append(values) => {
                        let extra = self.expand(values)?;
                        let n = self.arena_values(last)?.len();
                        self.arena_copy(last, n, &extra)?
                    }
                    K::Chop(count) => {
                        let values = self.arena_values(last)?;
                        let mut cut = values.len();
                        for _ in 0..*count {
                            self.work(1)?;
                            cut = cut.checked_sub(1).ok_or(bad("Chop underflow"))?;
                            if self.arena_values(last)?.get(cut) == Some(&Ty::WideTail) {
                                self.work(1)?;
                                cut = cut.checked_sub(1).ok_or(bad("Chop orphan tail"))?;
                                if self
                                    .arena_values(last)?
                                    .get(cut)
                                    .is_none_or(|v| v.width() != 2)
                                {
                                    return Err(bad("Chop invalid wide head"));
                                }
                            }
                        }
                        self.arena_copy(last, cut, &[])?
                    }
                };
                let pending = self
                    .arena
                    .get(locals)
                    .ok_or(bad("map prefix missing"))?
                    .this_count
                    != 0;
                reserve(&mut self.maps, 1, "prescribed maps")?;
                self.maps.push(Map {
                    offset: frame.bytecode_offset,
                    locals,
                    stack,
                    pending,
                    location: self.location,
                });
                last = locals;
            }
        }
        Ok(())
    }
    pub fn map_index(&mut self, offset: u32) -> T<usize> {
        self.work(16)?;
        self.maps
            .binary_search_by_key(&offset, |m| m.offset)
            .map_err(|_| bad("explicit control target requires a declared map"))
    }
    pub fn map_frame(&mut self, index: usize) -> T<Frame> {
        self.work(1)?;
        let map = self.maps.get(index).ok_or(bad("invalid map index"))?;
        let (locals, pending, n) = (map.locals, map.pending, map.stack.len());
        self.cells(n)?;
        let mut stack = Vec::new();
        reserve(&mut stack, n, "current map stack")?;
        stack.extend_from_slice(&self.maps.get(index).ok_or(bad("invalid map index"))?.stack);
        Ok(Frame {
            locals: Locals::Shared(locals),
            stack,
            pending,
        })
    }
    pub fn compare_map(
        &mut self,
        source: &Frame,
        index: usize,
        substitution: Option<(Ty, Ty)>,
    ) -> T<()> {
        self.compare_parts(
            &source.locals,
            &source.stack,
            source.pending,
            index,
            substitution,
        )
    }
    pub fn compare_parts(
        &mut self,
        source_locals: &Locals,
        source_stack: &[Ty],
        source_pending: bool,
        index: usize,
        substitution: Option<(Ty, Ty)>,
    ) -> T<()> {
        let map = self.maps.get(index).ok_or(bad("invalid comparison map"))?;
        let (dest, pending, nstack) = (map.locals, map.pending, map.stack.len());
        if source_pending && !pending {
            return Err(bad("constructor initialization flag is lost at a map"));
        }
        if source_stack.len() != nstack {
            return Err(bad("operand stack height differs from map"));
        }
        // A shared immutable arena identity proves this prefix equal without a
        // locals-times-Same-maps scan. Exceptional substituted views differ.
        let n = if substitution.is_none()
            && matches!(source_locals, Locals::Shared(id) if *id == dest)
        {
            0
        } else {
            self.local_values(source_locals)?
                .len()
                .max(self.arena_values(dest)?.len())
        };
        self.work(
            n.checked_add(nstack)
                .ok_or(bad("frame comparison size overflow"))?,
        )?;
        for i in 0..n {
            let mut from = self
                .local_values(source_locals)?
                .get(i)
                .copied()
                .unwrap_or(Ty::Top);
            if let Some((old, new)) = substitution {
                if from == old {
                    from = new;
                }
            }
            let to = self.arena_values(dest)?.get(i).copied().unwrap_or(Ty::Top);
            if !self.assign_paid(from, to)? {
                return Err(bad("local type differs from map"));
            }
        }
        for i in 0..nstack {
            let mut from = *source_stack.get(i).ok_or(bad("stack comparison missing"))?;
            if let Some((old, new)) = substitution {
                if from == old {
                    from = new;
                }
            }
            let to = *self
                .maps
                .get(index)
                .and_then(|m| m.stack.get(i))
                .ok_or(bad("map stack comparison missing"))?;
            if !self.assign_paid(from, to)? {
                return Err(bad("stack type differs from map"));
            }
        }
        Ok(())
    }
    pub fn substitute(&mut self, frame: &mut Frame, old: Ty, new: Ty) -> T<()> {
        let n = self
            .local_values(&frame.locals)?
            .len()
            .checked_add(frame.stack.len())
            .ok_or(bad("substitution size overflow"))?;
        self.work(n)?;
        self.replace_locals_paid(frame, old, new)?;
        for ty in frame.stack.iter_mut() {
            if *ty == old {
                *ty = new;
            }
        }
        Ok(())
    }
    pub fn substitute_locals(&mut self, frame: &mut Frame, old: Ty, new: Ty) -> T<()> {
        let n = self.local_values(&frame.locals)?.len();
        self.work(n)?;
        self.replace_locals_paid(frame, old, new)
    }
    fn replace_locals_paid(&mut self, frame: &mut Frame, old: Ty, new: Ty) -> T<()> {
        if let Locals::Shared(id) = frame.locals {
            let values = self.arena_values(id)?;
            if !values.contains(&old) {
                return Ok(());
            }
            let n = values.len();
            self.cells(n)?;
            let mut copy = Vec::new();
            reserve(&mut copy, n, "substituted current locals")?;
            for ty in self.arena_values(id)? {
                copy.push(if *ty == old { new } else { *ty });
            }
            frame.locals = Locals::Owned(copy);
            return Ok(());
        }
        let Locals::Owned(values) = &mut frame.locals else {
            return Err(bad("invalid local ownership"));
        };
        for ty in values {
            if *ty == old {
                *ty = new;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::World;
    use super::*;
    #[test]
    fn width_count_pass_is_funded_before_visiting_or_materializing_map_types() {
        let world = World::new(&[], &[]);
        let env = world.environment();
        let mut p = Proof::new(
            &env,
            world.core.object,
            TypeVerificationOptions {
                max_work_units: 1,
                ..Default::default()
            },
            CodeValidationUsage::default(),
        );
        let input = [V::Object(0), V::Long];
        assert!(matches!(
            p.expand(&input),
            Err(TypeVerificationErrorKind::ResourceLimit {
                resource: TypeVerificationResource::WorkUnits,
                required: 2,
                ..
            })
        ));
        assert_eq!(p.usage, TypeVerificationUsage::default());
        p.options.max_work_units = 5;
        assert_eq!(
            p.expand(&[V::Long, V::Top]).unwrap(),
            vec![Ty::Long, Ty::WideTail, Ty::Top]
        );
        assert_eq!(p.usage.state_cells, 3);
        assert_eq!(p.usage.work_units, 5);
    }
    #[test]
    fn nested_attribute_header_is_charged_before_body_inspection() {
        let world = World::new(&[], &[]);
        let env = world.environment();
        let mut p = Proof::new(
            &env,
            world.core.object,
            TypeVerificationOptions {
                max_work_units: 1,
                ..Default::default()
            },
            CodeValidationUsage::default(),
        );
        let attribute = crabspot_classfile::Attribute {
            offset: 0,
            name_index: 0,
            body: crabspot_classfile::AttributeBody::Synthetic,
        };
        let code = Code {
            max_stack: 0,
            max_locals: 0,
            code: vec![],
            exception_table: vec![],
            attributes: vec![attribute.clone(), attribute],
        };
        assert!(matches!(
            p.prepare_maps(&code, 0),
            Err(TypeVerificationErrorKind::ResourceLimit {
                resource: TypeVerificationResource::WorkUnits,
                required: 2,
                ..
            })
        ));
        assert_eq!(p.usage.work_units, 1);
        assert_eq!(p.usage.state_cells, 0);
        assert!(p.maps.is_empty());
    }
}
