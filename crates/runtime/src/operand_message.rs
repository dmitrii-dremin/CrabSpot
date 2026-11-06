//! Borrowed fixed recipes. Name comparison and emission are independently funded.
use super::*;
use crabspot_classfile::Constant;
#[derive(Clone, Copy)]
struct Recipe<'a> {
    rank: u8,
    leaf: Option<&'a [u16]>,
    primitive: Option<u16>,
    loader: LoaderId,
}
impl Recipe<'_> {
    fn len(self) -> R<u64> {
        let leaf = match self.leaf {
            Some(n) => u64::try_from(n.len()).map_err(|_| {
                error(OperandExceptionErrorKind::Overflow {
                    resource: OperandExceptionResource::MessageUnits,
                })
            })?,
            None => 1,
        };
        let wrappers = if self.rank > 0 && self.leaf.is_some() {
            2
        } else {
            0
        };
        u64::from(self.rank)
            .checked_add(leaf)
            .and_then(|n| n.checked_add(wrappers))
            .ok_or_else(|| {
                error(OperandExceptionErrorKind::Overflow {
                    resource: OperandExceptionResource::MessageUnits,
                })
            })
    }
    fn emit(self, out: &mut Vec<u16>) {
        for _ in 0..self.rank {
            out.push(u16::from(b'['));
        }
        if let Some(n) = self.leaf {
            if self.rank > 0 {
                out.push(u16::from(b'L'));
            }
            for &u in n {
                out.push(if u == u16::from(b'/') {
                    u16::from(b'.')
                } else {
                    u
                });
            }
            if self.rank > 0 {
                out.push(u16::from(b';'));
            }
        } else if let Some(tag) = self.primitive {
            out.push(tag);
        }
    }
}
struct Decimal {
    units: [u16; 11],
    start: usize,
}
impl Decimal {
    fn slice(&self) -> &[u16] {
        &self.units[self.start..]
    }
}
impl<'a> FieldResolutionContext<'a> {
    fn diagnostic_type(&mut self, ty: ReferenceType, describe: bool) -> R<Recipe<'a>> {
        self.operand_work(1)?;
        let (rank, leaf, loader, primitive) = match ty {
            ReferenceType::Class(id) => (0, Some(id), self.registry.defining_loader(id)?, None),
            ReferenceType::Array {
                dimensions,
                element,
                defining_loader,
            } => {
                if dimensions == 0 {
                    return Err(error(OperandExceptionErrorKind::InvalidFault));
                }
                self.registry.loader_index(defining_loader)?;
                let (id, tag) = match element {
                    ArrayElement::Class(id) => (Some(id), None),
                    ArrayElement::Primitive(p) => {
                        if Some(defining_loader)
                            != self
                                .heap
                                .reference_type_universe()
                                .map(|u| u.bootstrap_loader)
                        {
                            return Err(error(OperandExceptionErrorKind::InvalidFault));
                        }
                        (
                            None,
                            Some(u16::from(match p {
                                PrimitiveArrayKind::Boolean => b'Z',
                                PrimitiveArrayKind::Byte => b'B',
                                PrimitiveArrayKind::Char => b'C',
                                PrimitiveArrayKind::Short => b'S',
                                PrimitiveArrayKind::Int => b'I',
                                PrimitiveArrayKind::Long => b'J',
                                PrimitiveArrayKind::Float => b'F',
                                PrimitiveArrayKind::Double => b'D',
                            })),
                        )
                    }
                };
                (dimensions, id, defining_loader, tag)
            }
        };
        let mut units = None;
        if let Some(id) = leaf {
            self.operand_work(1)?;
            let c = self.registry.class(id)?;
            self.heap.instance_graph_node(id)?;
            if self.registry.defining_loader(id)? != loader {
                return Err(error(OperandExceptionErrorKind::InvalidFault));
            }
            if describe {
                self.operand_work(1)?;
                let Constant::Class { name_index } = c
                    .constant_pool
                    .get(c.this_class)
                    .map_err(RegistryError::Parse)?
                else {
                    return Err(invalid("diagnostic self Class differs"));
                };
                self.operand_work(1)?;
                units = Some(
                    c.constant_pool
                        .utf8(*name_index)
                        .map_err(RegistryError::Parse)?
                        .units(),
                );
            }
        }
        Ok(Recipe {
            rank,
            leaf: units,
            primitive,
            loader,
        })
    }
    fn decimal(&mut self, n: i32) -> R<Decimal> {
        let mut result = Decimal {
            units: [0; 11],
            start: 11,
        };
        let mut value = n.unsigned_abs();
        loop {
            self.operand_work(1)?;
            result.start -= 1;
            result.units[result.start] = u16::from(b'0') + (value % 10) as u16;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        if n < 0 {
            self.operand_work(1)?;
            result.start -= 1;
            result.units[result.start] = u16::from(b'-');
        }
        Ok(result)
    }
    fn same_diagnostic_name(&mut self, a: Recipe<'_>, b: Recipe<'_>) -> R<bool> {
        if a.rank != b.rank || a.primitive != b.primitive {
            return Ok(false);
        }
        match (a.leaf, b.leaf) {
            (Some(a), Some(b)) if a.len() == b.len() => {
                self.operand_work(u64::try_from(a.len()).map_err(|_| {
                    error(OperandExceptionErrorKind::Overflow {
                        resource: OperandExceptionResource::DiagnosticWork,
                    })
                })?)?;
                Ok(a == b)
            }
            (None, None) => Ok(true),
            _ => Ok(false),
        }
    }
    /// Construct one fresh managed message; no interning or guest method calls.
    pub fn new_operand_exception_message(
        &mut self,
        claim: &InitializationClaim,
        spec: &OperandExceptionSpec,
    ) -> R<Reference> {
        self.check_operand_spec(claim, spec)?;
        self.operand_work(1)?;
        let mut first = None;
        let mut second = None;
        let mut a = None;
        let mut b = None;
        let mut suffix = false;
        let len = match spec.fault {
            OperandExceptionFault::Bounds { index, length } => {
                let x = self.decimal(index)?;
                let y = self.decimal(length)?;
                let n = 32 + x.slice().len() as u64 + y.slice().len() as u64;
                first = Some(x);
                second = Some(y);
                n
            }
            OperandExceptionFault::NegativeSize { length, .. } => {
                let x = self.decimal(length)?;
                let n = x.slice().len() as u64;
                first = Some(x);
                n
            }
            OperandExceptionFault::Store { source, component } => {
                let source = self.diagnostic_type(source, true)?;
                self.diagnostic_type(component, false)?;
                let n = source.len()?;
                a = Some(source);
                n
            }
            OperandExceptionFault::Cast { source, target } => {
                let source = self.diagnostic_type(source, true)?;
                let target = self.diagnostic_type(target, true)?;
                suffix =
                    self.same_diagnostic_name(source, target)? && source.loader != target.loader;
                let source_len = source.len()?;
                let target_len = target.len()?;
                let n = 31u64
                    .checked_add(source_len)
                    .and_then(|n| n.checked_add(target_len))
                    .and_then(|n| n.checked_add(if suffix { 29 } else { 0 }))
                    .ok_or_else(|| {
                        error(OperandExceptionErrorKind::Overflow {
                            resource: OperandExceptionResource::MessageUnits,
                        })
                    })?;
                a = Some(source);
                b = Some(target);
                n
            }
        };
        let (mut out, messages) = self
            .heap
            .operand_exceptions
            .as_mut()
            .ok_or_else(|| error(OperandExceptionErrorKind::Disabled))?
            .message_buffer(len)?;
        let size = usize::try_from(len).map_err(|_| {
            error(OperandExceptionErrorKind::Overflow {
                resource: OperandExceptionResource::TemporaryBytes,
            })
        })?;
        match spec.fault {
            OperandExceptionFault::Bounds { .. } => {
                ascii(&mut out, "Index ");
                if let Some(x) = &first {
                    out.extend_from_slice(x.slice());
                }
                ascii(&mut out, " out of bounds for length ");
                if let Some(y) = &second {
                    out.extend_from_slice(y.slice());
                }
            }
            OperandExceptionFault::NegativeSize { .. } => {
                if let Some(x) = &first {
                    out.extend_from_slice(x.slice());
                }
            }
            OperandExceptionFault::Store { .. } => {
                if let Some(x) = a {
                    x.emit(&mut out);
                }
            }
            OperandExceptionFault::Cast { .. } => {
                ascii(&mut out, "class ");
                if let Some(x) = a {
                    x.emit(&mut out);
                }
                ascii(&mut out, " cannot be cast to class ");
                if let Some(y) = b {
                    y.emit(&mut out);
                }
                if suffix {
                    ascii(&mut out, " (different defining loaders)");
                }
            }
        }
        if out.len() != size {
            return Err(error(OperandExceptionErrorKind::InvalidState(
                "diagnostic recipe length differs",
            )));
        }
        let reference = self.fresh_string(&out)?;
        // fresh_string cannot change operand binding; no fallible lookup follows its publication.
        if let Some(state) = self.heap.operand_exceptions.as_mut() {
            state.usage.messages = messages;
        }
        Ok(reference)
    }
}
fn ascii(out: &mut Vec<u16>, s: &str) {
    out.extend(s.bytes().map(u16::from));
}

impl OperandState {
    pub(super) fn message_buffer(&mut self, len: u64) -> R<(Vec<u16>, u64)> {
        checked(
            0,
            len,
            self.limits.max_message_units,
            OperandExceptionResource::MessageUnits,
        )?;
        let total = checked(
            self.usage.message_units,
            len,
            self.limits.max_total_message_units,
            OperandExceptionResource::TotalMessageUnits,
        )?;
        let bytes = len.checked_mul(2).ok_or_else(|| {
            error(OperandExceptionErrorKind::Overflow {
                resource: OperandExceptionResource::TemporaryBytes,
            })
        })?;
        checked(
            0,
            bytes,
            self.limits.max_temporary_bytes,
            OperandExceptionResource::TemporaryBytes,
        )?;
        let work = checked(
            self.usage.work_units,
            len,
            self.limits.max_work_units,
            OperandExceptionResource::DiagnosticWork,
        )?;
        let messages = checked(
            self.usage.messages,
            1,
            u64::MAX,
            OperandExceptionResource::Messages,
        )?;
        let size = usize::try_from(len).map_err(|_| {
            error(OperandExceptionErrorKind::Overflow {
                resource: OperandExceptionResource::TemporaryBytes,
            })
        })?;
        self.usage.work_units = work;
        self.usage.message_units = total;
        let mut out = Vec::new();
        if size != 0 {
            self.reserving()?;
            out.try_reserve_exact(size)
                .map_err(|_| error(OperandExceptionErrorKind::AllocationFailure))?;
        }
        Ok((out, messages))
    }
}
