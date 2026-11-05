use super::*;
struct Insertion {
    name: usize,
    hash: u64,
    new_name: Option<Vec<u16>>,
    bucket: Option<Vec<usize>>,
    added: [Option<(LoaderId, usize)>; 2],
    usage: LoadingDomainUsage,
    before: LoadingDomainUsage,
    changes: (u64, u64),
}
pub(super) struct Binding {
    insertion: Insertion,
    pair: usize,
    root: usize,
}
impl Ledger {
    #[cfg(test)]
    pub(super) fn prospective_for_test(
        &self,
        names: u64,
        units: u64,
        pairs: u64,
        initiations: u64,
        unions: u64,
    ) -> Result<LoadingDomainUsage> {
        self.prospective(names, units, pairs, initiations, unions)
    }
    pub(super) fn name_id(&self, name: &[u16], work: &mut Work) -> Result<(Option<usize>, u64)> {
        work.step()?;
        let hash = self.hash.hash_one(name);
        if let Some(bucket) = self.name_index.get(&hash) {
            for id in bucket {
                work.step()?;
                let stored = self
                    .names
                    .get(*id)
                    .ok_or(invalid("invalid retained name index"))?;
                if equal_paid(work, name, stored)? {
                    return Ok((Some(*id), hash));
                }
            }
        }
        Ok((None, hash))
    }
    fn pair_id(&self, name: usize, loader: LoaderId, work: &mut Work) -> Result<Option<usize>> {
        work.step()?;
        Ok(self.pair_index.get(&(name, loader)).copied())
    }
    fn root(&self, mut id: usize, work: &mut Work) -> Result<usize> {
        for _ in 0..=self.pairs.len() {
            work.step()?;
            let node = self
                .pairs
                .get(id)
                .ok_or(invalid("invalid union parent index"))?;
            if node.size == 0 || node.size > self.pairs.len() {
                return Err(invalid("zero union size"));
            }
            if node.parent == id {
                return Ok(id);
            }
            id = node.parent;
        }
        Err(invalid("cyclic union parents"))
    }
    pub(super) fn lookup(
        &self,
        name: &[u16],
        loader: LoaderId,
        work: &mut Work,
    ) -> Result<Option<ClassId>> {
        let query = self.name_id(name, work)?;
        self.lookup_queried(query, loader, work)
    }
    pub(super) fn lookup_queried(
        &self,
        query: (Option<usize>, u64),
        loader: LoaderId,
        work: &mut Work,
    ) -> Result<Option<ClassId>> {
        let (id, _) = query;
        let Some(id) = id else { return Ok(None) };
        let Some(pair) = self.pair_id(id, loader, work)? else {
            return Ok(None);
        };
        Ok(self
            .pairs
            .get(pair)
            .ok_or(invalid("invalid actual initiation index"))?
            .actual)
    }
    pub(super) fn constrained(
        &self,
        name: &[u16],
        left: LoaderId,
        right: LoaderId,
        work: &mut Work,
    ) -> Result<bool> {
        if left == right {
            return Ok(true);
        }
        let (id, _) = self.name_id(name, work)?;
        let Some(id) = id else { return Ok(false) };
        let left = self.pair_id(id, left, work)?;
        let right = self.pair_id(id, right, work)?;
        match (left, right) {
            (Some(a), Some(b)) => Ok(self.root(a, work)? == self.root(b, work)?),
            _ => Ok(false),
        }
    }
    fn prospective(
        &self,
        names: u64,
        units: u64,
        pairs: u64,
        initiations: u64,
        unions: u64,
    ) -> Result<LoadingDomainUsage> {
        let old = self.usage;
        let limits = self.limits;
        let all = [
            (old.names, names, limits.max_names, R::Names),
            (old.name_units, units, limits.max_name_units, R::NameUnits),
            (old.pairs, pairs, limits.max_pairs, R::Pairs),
            (
                old.initiations,
                initiations,
                limits.max_initiations,
                R::Initiations,
            ),
            (old.unions, unions, limits.max_unions, R::Unions),
        ];
        let mut next = [0; 5];
        for (dst, (old, n, limit, resource)) in next.iter_mut().zip(all) {
            *dst = old
                .checked_add(n)
                .ok_or(E::AccountingOverflow { resource })?;
            if *dst > limit {
                return Err(E::ResourceLimit {
                    resource,
                    limit,
                    required: *dst,
                });
            }
        }
        Ok(LoadingDomainUsage {
            names: next[0],
            name_units: next[1],
            pairs: next[2],
            initiations: next[3],
            unions: next[4],
        })
    }
    fn stage(
        &mut self,
        name: &[u16],
        name_id: Option<usize>,
        hash: u64,
        loaders: &[LoaderId],
        changes: (u64, u64),
        work: &mut Work,
    ) -> Result<Insertion> {
        if loaders.len() > 2 {
            return Err(invalid("oversized insertion staging"));
        }
        work.step()?;
        let fresh = name_id.is_none();
        let usage = self.prospective(
            u64::from(fresh),
            if fresh {
                amount(name.len(), R::NameUnits)?
            } else {
                0
            },
            amount(loaders.len(), R::Pairs)?,
            changes.0,
            changes.1,
        )?;
        let id = name_id.unwrap_or(self.names.len());
        let mut new_name = None;
        let mut bucket = None;
        if fresh {
            work.utf(name.len())?;
            let mut copy = Vec::new();
            work.reserving()?;
            reserve(&mut copy, name.len())?;
            copy.extend_from_slice(name);
            work.reserving()?;
            reserve(&mut self.names, 1)?;
            // Both staging and commit scalar index probes are prepaid. Rehash keys are
            // fixed fingerprints; retained UTF-16 never participates in table growth.
            work.charge(0, 2, 0)?;
            if let Some(existing) = self.name_index.get_mut(&hash) {
                work.reserving()?;
                reserve(existing, 1)?;
            } else {
                work.reserving()?;
                self.name_index
                    .try_reserve(1)
                    .map_err(|_| E::AllocationFailure)?;
                let mut new = Vec::new();
                work.reserving()?;
                reserve(&mut new, 1)?;
                bucket = Some(new)
            }
            new_name = Some(copy);
        }
        if !loaders.is_empty() {
            work.reserving()?;
            reserve(&mut self.pairs, loaders.len())?;
            work.charge(0, amount(loaders.len(), R::WorkUnits)?, 0)?;
            work.reserving()?;
            self.pair_index
                .try_reserve(loaders.len())
                .map_err(|_| E::AllocationFailure)?;
        }
        let mut added = [None; 2];
        for (index, slot) in added.iter_mut().take(loaders.len()).enumerate() {
            *slot = Some((
                loaders[index],
                self.pairs
                    .len()
                    .checked_add(index)
                    .ok_or(E::AccountingOverflow { resource: R::Pairs })?,
            ));
        }
        Ok(Insertion {
            name: id,
            hash,
            new_name,
            bucket,
            added,
            usage,
            before: self.usage,
            changes,
        })
    }
    fn commit(&mut self, insertion: Insertion) {
        let Insertion {
            name,
            hash,
            new_name,
            bucket,
            added,
            usage,
            before: _,
            changes: _,
        } = insertion;
        if let Some(spelling) = new_name {
            self.names.push(spelling);
            if let Some(mut bucket) = bucket {
                bucket.push(name);
                self.name_index.insert(hash, bucket);
            } else if let Some(bucket) = self.name_index.get_mut(&hash) {
                bucket.push(name);
            }
        }
        for (loader, id) in added.into_iter().flatten() {
            self.pairs.push(Pair {
                parent: id,
                size: 1,
                known: None,
                actual: None,
            });
            self.pair_index.insert((name, loader), id);
        }
        self.usage = usage;
    }
    pub(super) fn bind(
        &mut self,
        name: &[u16],
        loader: LoaderId,
        class: Option<ClassId>,
        work: &mut Work,
    ) -> Result<Option<Binding>> {
        let query = self.name_id(name, work)?;
        self.bind_queried(name, loader, class, query, work)
    }
    pub(super) fn bind_queried(
        &mut self,
        name: &[u16],
        loader: LoaderId,
        class: Option<ClassId>,
        query: (Option<usize>, u64),
        work: &mut Work,
    ) -> Result<Option<Binding>> {
        let (id, hash) = query;
        let pair = if let Some(id) = id {
            self.pair_id(id, loader, work)?
        } else {
            None
        };
        let root = if let Some(pair) = pair {
            Some(self.root(pair, work)?)
        } else {
            None
        };
        if let Some(pair) = pair {
            let actual = self
                .pairs
                .get(pair)
                .ok_or(invalid("invalid bind pair"))?
                .actual;
            if let Some(existing) = actual {
                work.step()?;
                if root.and_then(|id| self.pairs.get(id)).and_then(|p| p.known) != Some(existing) {
                    return Err(invalid("actual record disagrees with component identity"));
                }
                if let Some(incoming) = class {
                    if incoming != existing {
                        return Err(E::InitiationConflict {
                            loader,
                            existing,
                            incoming,
                        });
                    }
                }
                return Ok(None);
            }
        }
        if let Some(root) = root {
            if let Some(existing) = self
                .pairs
                .get(root)
                .ok_or(invalid("invalid bind root"))?
                .known
            {
                match class {
                    Some(incoming) if incoming == existing => {}
                    Some(incoming) => {
                        return Err(E::InitiationConflict {
                            loader,
                            existing,
                            incoming,
                        });
                    }
                    None => {
                        return Err(E::DefinitionConflict {
                            loader,
                            required: existing,
                        });
                    }
                }
            }
        }
        let loaders = if pair.is_none() { Some(loader) } else { None };
        let insertion = self.stage(name, id, hash, loaders.as_slice(), (1, 0), work)?;
        let pair = pair.unwrap_or(self.pairs.len());
        let binding = Binding {
            insertion,
            pair,
            root: root.unwrap_or(pair),
        };
        self.validate_binding(&binding, class, work)?;
        Ok(Some(binding))
    }
    pub(super) fn commit_bind(&mut self, binding: Binding, class: ClassId) {
        let Binding {
            insertion,
            pair,
            root,
        } = binding;
        self.commit(insertion);
        if let Some(node) = self.pairs.get_mut(pair) {
            node.actual = Some(class)
        }
        if let Some(node) = self.pairs.get_mut(root) {
            node.known = Some(class)
        }
    }
    pub(super) fn impose(
        &mut self,
        name: &[u16],
        left: LoaderId,
        right: LoaderId,
        work: &mut Work,
    ) -> Result<()> {
        if left == right {
            return Ok(());
        }
        let (id, hash) = self.name_id(name, work)?;
        let a = if let Some(id) = id {
            self.pair_id(id, left, work)?
        } else {
            None
        };
        let b = if let Some(id) = id {
            self.pair_id(id, right, work)?
        } else {
            None
        };
        let ar = if let Some(a) = a {
            Some(self.root(a, work)?)
        } else {
            None
        };
        let br = if let Some(b) = b {
            Some(self.root(b, work)?)
        } else {
            None
        };
        if ar.is_some() && ar == br {
            return Ok(());
        }
        let ak = ar.and_then(|r| self.pairs.get(r)).and_then(|p| p.known);
        let bk = br.and_then(|r| self.pairs.get(r)).and_then(|p| p.known);
        if let (Some(left_class), Some(right_class)) = (ak, bk) {
            if left_class != right_class {
                return Err(E::ConstraintConflict {
                    left,
                    right,
                    left_class,
                    right_class,
                });
            }
        }
        let mut missing = [left; 2];
        let mut n = 0;
        if a.is_none() {
            missing[n] = left;
            n += 1
        }
        if b.is_none() {
            missing[n] = right;
            n += 1
        }
        let future = self.pairs.len();
        let a = ar.unwrap_or(future);
        let b = br.unwrap_or(
            future
                .checked_add(usize::from(ar.is_none()))
                .ok_or(E::AccountingOverflow { resource: R::Pairs })?,
        );
        let size_a = ar.and_then(|r| self.pairs.get(r)).map_or(1, |p| p.size);
        let size_b = br.and_then(|r| self.pairs.get(r)).map_or(1, |p| p.size);
        let size = size_a
            .checked_add(size_b)
            .ok_or(E::AccountingOverflow { resource: R::Pairs })?;
        let (winner, loser) = if size_a > size_b || (size_a == size_b && a < b) {
            (a, b)
        } else {
            (b, a)
        };
        let staged = self.stage(name, id, hash, &missing[..n], (0, 1), work)?;
        self.validate_join(&staged, winner, loser, size, work)?;
        self.commit(staged);
        if let Some(node) = self.pairs.get_mut(loser) {
            node.parent = winner
        }
        if let Some(node) = self.pairs.get_mut(winner) {
            node.size = size;
            node.known = ak.or(bk)
        }
        Ok(())
    }
}

impl Ledger {
    fn validate_stage(&self, stage: &Insertion, work: &mut Work) -> Result<()> {
        if self.usage != stage.before {
            return Err(invalid("stale insertion usage"));
        }
        if amount(self.names.len(), R::Names)? != self.usage.names
            || amount(self.pairs.len(), R::Pairs)? != self.usage.pairs
            || self.usage.initiations > self.usage.pairs
        {
            return Err(invalid("inconsistent retained table counts"));
        }
        let fresh = stage.new_name.is_some();
        if fresh {
            let spelling = stage
                .new_name
                .as_deref()
                .ok_or(invalid("missing staged spelling"))?;
            if stage.name != self.names.len() || spelling.len() > usize::from(u16::MAX) {
                return Err(invalid("invalid staged name position"));
            }
            work.name(spelling)?;
            let (found, hash) = self.name_id(spelling, work)?;
            if found.is_some() || hash != stage.hash {
                return Err(invalid("staged spelling no longer absent"));
            }
            work.step()?;
            match (&stage.bucket, self.name_index.get(&stage.hash)) {
                (Some(bucket), None)
                    if bucket.is_empty()
                        && bucket.capacity() > 0
                        && self.name_index.capacity() > self.name_index.len() => {}
                (None, Some(bucket)) if bucket.capacity() > bucket.len() => {}
                _ => return Err(invalid("missing staged name bucket capacity")),
            }
            if self.names.capacity() <= self.names.len() {
                return Err(invalid("missing staged name arena capacity"));
            }
        } else if stage.name >= self.names.len() || stage.bucket.is_some() {
            return Err(invalid("invalid existing staged name"));
        }
        let mut count = 0;
        let mut missing = false;
        let mut prior = None;
        for item in stage.added {
            let Some((loader, id)) = item else {
                missing = true;
                continue;
            };
            if missing || prior == Some(loader) {
                return Err(invalid("invalid staged loader slots"));
            }
            prior = Some(loader);
            let expected = self
                .pairs
                .len()
                .checked_add(count)
                .ok_or(E::AccountingOverflow { resource: R::Pairs })?;
            if id != expected {
                return Err(invalid("invalid future pair position"));
            }
            work.step()?;
            if self.pair_index.contains_key(&(stage.name, loader)) {
                return Err(invalid("staged pair already exists"));
            }
            count += 1;
        }
        if self
            .pairs
            .capacity()
            .checked_sub(self.pairs.len())
            .is_none_or(|n| n < count)
            || self
                .pair_index
                .capacity()
                .checked_sub(self.pair_index.len())
                .is_none_or(|n| n < count)
        {
            return Err(invalid("missing staged pair capacity"));
        }
        let units = stage.new_name.as_ref().map_or(0, Vec::len);
        if self.prospective(
            u64::from(fresh),
            amount(units, R::NameUnits)?,
            amount(count, R::Pairs)?,
            stage.changes.0,
            stage.changes.1,
        )? != stage.usage
        {
            return Err(invalid("invalid staged retained totals"));
        }
        Ok(())
    }
    fn validate_binding(
        &self,
        binding: &Binding,
        class: Option<ClassId>,
        work: &mut Work,
    ) -> Result<()> {
        self.validate_stage(&binding.insertion, work)?;
        if binding.insertion.changes != (1, 0) {
            return Err(invalid("invalid bind increments"));
        }
        if binding.pair < self.pairs.len() {
            work.step()?;
            let pair = self
                .pairs
                .get(binding.pair)
                .ok_or(invalid("missing bind pair"))?;
            if pair.actual.is_some() || self.root(binding.pair, work)? != binding.root {
                return Err(invalid("changed bind pair"));
            }
            work.step()?;
            let root = self
                .pairs
                .get(binding.root)
                .ok_or(invalid("missing bind root"))?;
            if root.parent != binding.root || root.known.is_some() && root.known != class {
                return Err(invalid("changed bind identity"));
            }
            if binding.insertion.added.iter().any(Option::is_some) {
                return Err(invalid("existing bind adds unrelated pair"));
            }
        } else if binding.root != binding.pair
            || binding.insertion.added[0].is_none_or(|(_, id)| id != binding.pair)
            || binding.insertion.added[1].is_some()
        {
            return Err(invalid("invalid fresh bind destination"));
        }
        Ok(())
    }
    fn staged_root(
        &self,
        stage: &Insertion,
        id: usize,
        work: &mut Work,
    ) -> Result<(usize, Option<ClassId>)> {
        work.step()?;
        if let Some(node) = self.pairs.get(id) {
            if node.parent != id || node.size == 0 || node.size > self.pairs.len() {
                return Err(invalid("join destination is not a root"));
            }
            return Ok((node.size, node.known));
        }
        if stage
            .added
            .iter()
            .flatten()
            .any(|(_, future)| *future == id)
        {
            return Ok((1, None));
        }
        Err(invalid("join future root missing"))
    }
    fn validate_join(
        &self,
        stage: &Insertion,
        winner: usize,
        loser: usize,
        size: usize,
        work: &mut Work,
    ) -> Result<()> {
        self.validate_stage(stage, work)?;
        if winner == loser || stage.changes != (0, 1) {
            return Err(invalid("invalid join increments"));
        }
        let (ws, wk) = self.staged_root(stage, winner, work)?;
        let (ls, lk) = self.staged_root(stage, loser, work)?;
        if ws < ls
            || ws == ls && winner > loser
            || ws.checked_add(ls) != Some(size)
            || wk.is_some() && lk.is_some() && wk != lk
        {
            return Err(invalid("invalid staged union relationship"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{object, snapshot};
    use super::*;
    fn work() -> Work {
        Work {
            limits: LoadingWorkLimits::default(),
            usage: LoadingWorkUsage::default(),
            site: None,
            fail_reserve_after: None,
        }
    }
    fn domain() -> LoadingDomain {
        LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap()
    }
    #[test]
    fn forged_bind_stages_reject_every_future_destination_and_capacity_without_publication() {
        for damage in 0..9 {
            let mut d = domain();
            let a = d.create_loader().unwrap();
            let class = object(&mut d, a);
            let b = d.create_loader().unwrap();
            let mut w = work();
            let name: Vec<_> = "p/New".encode_utf16().collect();
            w.name(&name).unwrap();
            let mut binding = d
                .ledger
                .bind(&name, b, Some(class), &mut w)
                .unwrap()
                .unwrap();
            let before = snapshot(&d.ledger);
            match damage {
                0 => binding.root = usize::MAX,
                1 => binding.pair = usize::MAX,
                2 => binding.insertion.name = usize::MAX,
                3 => binding.insertion.added[0] = Some((b, usize::MAX)),
                4 => binding.insertion.usage.initiations += 1,
                5 => binding.insertion.before.unions += 1,
                6 => binding.insertion.bucket = Some(Vec::new()),
                7 => binding.insertion.new_name = Some(vec![46]),
                _ => binding.insertion.added[1] = binding.insertion.added[0],
            }
            assert!(
                d.ledger
                    .validate_binding(&binding, Some(class), &mut w)
                    .is_err()
            );
            assert_eq!(snapshot(&d.ledger), before);
        }
    }
    #[test]
    fn forged_join_roots_sizes_and_counters_never_change_an_existing_component() {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        let c = d.create_loader().unwrap();
        object(&mut d, a);
        let name: Vec<_> = "p/K".encode_utf16().collect();
        let mut w = work();
        w.name(&name).unwrap();
        d.ledger.impose(&name, a, b, &mut w).unwrap();
        let (id, hash) = d.ledger.name_id(&name, &mut w).unwrap();
        let existing = d.ledger.pair_id(id.unwrap(), a, &mut w).unwrap().unwrap();
        let root = d.ledger.root(existing, &mut w).unwrap();
        let future = d.ledger.pairs.len();
        let stage = d
            .ledger
            .stage(&name, id, hash, &[c], (0, 1), &mut w)
            .unwrap();
        let before = snapshot(&d.ledger);
        assert!(
            d.ledger
                .validate_join(&stage, root, usize::MAX, 3, &mut w)
                .is_err()
        );
        assert!(
            d.ledger
                .validate_join(&stage, root, future, usize::MAX, &mut w)
                .is_err()
        );
        assert!(
            d.ledger
                .validate_join(&stage, future, root, 3, &mut w)
                .is_err()
        );
        assert_eq!(snapshot(&d.ledger), before);
        d.ledger
            .validate_join(&stage, root, future, 3, &mut w)
            .unwrap();
    }
    #[test]
    fn corrupt_union_parents_and_known_identity_fail_with_paid_bounded_walks() {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let class = object(&mut d, a);
        let mut w = work();
        let id = d.ledger.pair_index.values().copied().next().unwrap();
        d.ledger.pairs[id].parent = usize::MAX;
        let before = snapshot(&d.ledger);
        assert!(d.ledger.root(id, &mut w).is_err());
        assert_eq!(snapshot(&d.ledger), before);
        d.ledger.pairs[id].parent = id;
        d.ledger.pairs[id].known = None;
        let n: Vec<_> = "java/lang/Object".encode_utf16().collect();
        w.name(&n).unwrap();
        assert!(d.ledger.bind(&n, a, Some(class), &mut w).is_err());
    }
    #[test]
    fn long_fingerprint_collision_comparisons_are_funded_before_reading_each_candidate() {
        let mut ledger = Ledger::new(LoadingDomainLimits::default());
        let input = vec![65; 65_535];
        let mut different = input.clone();
        different[65_534] = 66;
        ledger.names.push(different);
        ledger.names.push(input.clone());
        ledger
            .name_index
            .insert(ledger.hash.hash_one(input.as_slice()), vec![0, 1]);
        let mut w = work();
        w.limits.max_utf16_units = 65_535;
        let before = snapshot(&ledger);
        assert!(matches!(
            ledger.name_id(&input, &mut w),
            Err(E::ResourceLimit {
                resource: R::Utf16Units,
                required: 131_070,
                ..
            })
        ));
        assert_eq!(w.usage.utf16_units, 65_535);
        assert_eq!(snapshot(&ledger), before);
        w.limits.max_utf16_units = 200_000;
        assert_eq!(ledger.name_id(&input, &mut w).unwrap().0, Some(1));
    }
    #[test]
    fn deep_flat_union_walk_and_drop_are_iterative_and_low_work_rejects_early() {
        let mut ledger = Ledger::new(LoadingDomainLimits::default());
        for id in 0usize..32_000 {
            ledger.pairs.push(Pair {
                parent: id.saturating_sub(1),
                size: 1,
                known: None,
                actual: None,
            });
        }
        let mut w = work();
        w.limits.max_work_units = 4;
        assert!(ledger.root(31_999, &mut w).is_err());
        assert_eq!(w.usage.work_units, 4);
        w.limits.max_work_units = 40_000;
        assert_eq!(ledger.root(31_999, &mut w).unwrap(), 0);
        assert_eq!(ledger.pairs[31_999].parent, 31_998);
    }
}
