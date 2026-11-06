//! Streaming checked nest access. Domain opt-in precedes terminal member sites;
//! only immutable Java determinations become terminal member outcomes.
use crate::*;
use crabspot_classfile::{AttributeBody, Constant, JavaString};
impl<'a> FieldResolutionContext<'a> {
    pub(crate) fn nest_access_enabled(&self) -> bool {
        self.member.as_ref().is_some_and(|a| a.store.nest_access)
    }
    fn nest_name(&mut self, class: ClassId) -> Result<&'a JavaString, FieldAccessError> {
        let definition = self.registry().class(class)?;
        self.step()?;
        let Constant::Class { name_index } = definition
            .constant_pool
            .get(definition.this_class)
            .map_err(RegistryError::Parse)?
        else {
            return Err(FieldAccessError::InvalidState(
                "nest class has no self name",
            ));
        };
        self.step()?;
        Ok(definition
            .constant_pool
            .utf8(*name_index)
            .map_err(RegistryError::Parse)?)
    }
    pub(crate) fn nest_same_package(
        &mut self,
        a: ClassId,
        b: ClassId,
    ) -> Result<bool, FieldAccessError> {
        if self.registry().defining_loader(a)? != self.registry().defining_loader(b)? {
            return Ok(false);
        }
        let a = self.nest_name(a)?.units();
        let b = self.nest_name(b)?.units();
        self.charge_bulk_work(a.len(), 0)?;
        let end_a = a.iter().rposition(|x| *x == 47).unwrap_or(0);
        self.charge_bulk_work(b.len(), 0)?;
        let end_b = b.iter().rposition(|x| *x == 47).unwrap_or(0);
        if end_a != end_b {
            return Ok(false);
        }
        self.charge_bulk_work(end_a, 0)?;
        Ok(a.get(..end_a) == b.get(..end_b))
    }
    fn nest_host(&mut self, class: ClassId) -> Result<ClassId, FieldAccessError> {
        let definition = self.registry().class(class)?;
        let mut host = None;
        for attribute in &definition.attributes {
            self.step()?;
            if let AttributeBody::NestHost(index) = attribute.body {
                host = Some(index);
                break;
            }
        }
        let Some(index) = host else {
            return Ok(class);
        };
        let resolved = match self.resolve_class_mirror_type(class, index) {
            Ok(ReferenceType::Class(h)) => h,
            Ok(ReferenceType::Array { .. }) => return Ok(class),
            Err(e) if matches!(*e, ClassMirrorError::IllegalAccess) => return Ok(class),
            Err(e) => return Err(FieldAccessError::ClassMirror(e)),
        };
        if !self.nest_same_package(class, resolved)? {
            return Ok(class);
        }
        let host = self.registry().class(resolved)?;
        for attribute in &host.attributes {
            self.step()?;
            let AttributeBody::NestMembers(indices) = &attribute.body else {
                continue;
            };
            let name = self.nest_name(class)?;
            for &entry in indices {
                self.step()?; // list row
                self.step()?; // Class handle
                let Constant::Class { name_index } = host
                    .constant_pool
                    .get(entry)
                    .map_err(RegistryError::Parse)?
                else {
                    return Err(FieldAccessError::InvalidState(
                        "nest member entry is not Class",
                    ));
                };
                self.step()?; // Utf8 handle; do not resolve unrelated listed types
                let member = host
                    .constant_pool
                    .utf8(*name_index)
                    .map_err(RegistryError::Parse)?;
                if member.units().len() != name.units().len() {
                    continue;
                }
                self.charge_bulk_work(name.units().len(), 0)?;
                if member == name {
                    return Ok(resolved);
                }
            }
            return Ok(class);
        }
        Ok(class)
    }
    pub(crate) fn same_nest(
        &mut self,
        origin: ClassId,
        declaring: ClassId,
    ) -> Result<bool, FieldAccessError> {
        if origin == declaring {
            return Ok(true);
        }
        let d = self.nest_host(origin)?;
        let c = self.nest_host(declaring)?;
        Ok(d == c)
    }
}
