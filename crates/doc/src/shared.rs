//! [`Shared`]: a document's part that copies of the document share until one of them changes it.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A value shared by the copies of a document until one of them changes it: reading goes through
/// [`Deref`], and changing it ([`DerefMut`]) copies it first when another copy holds it too.
/// Copying a document (every undo step, every action of a prototype's run) copied its styles and
/// variables before (performance budget PF2); it is the same value either way, written and read
/// as the value itself.
#[derive(Clone, Default)]
pub struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Shared<T> {
        Shared(Arc::new(value))
    }

    /// Whether `a` and `b` are one value held twice (then they are equal without comparing).
    pub fn ptr_eq(a: &Shared<T>, b: &Shared<T>) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }
}

impl<T: Clone> Shared<T> {
    /// The value, taken out (copied when another copy holds it too).
    pub fn into_inner(self) -> T {
        Arc::unwrap_or_clone(self.0)
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone> DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Shared<T> {
        Shared::new(value)
    }
}

/// Compared by value, always: a value held once compares as it did before it was shared (one that
/// isn't equal to itself, a NaN number, still isn't).
impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Shared<T>) -> bool {
        *self.0 == *other.0
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Shared<T>, D::Error> {
        T::deserialize(d).map(Shared::new)
    }
}

impl<A, T: FromIterator<A>> FromIterator<A> for Shared<T> {
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Shared<T> {
        Shared::new(iter.into_iter().collect())
    }
}

impl<'a, T> IntoIterator for &'a Shared<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&*self.0).into_iter()
    }
}

impl<'a, T: Clone> IntoIterator for &'a mut Shared<T>
where
    &'a mut T: IntoIterator,
{
    type Item = <&'a mut T as IntoIterator>::Item;
    type IntoIter = <&'a mut T as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        Arc::make_mut(&mut self.0).into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::Shared;
    use crate::{Document, Variables};

    #[test]
    fn copies_share_the_value_until_one_changes_it() {
        let a = Shared::new(vec![1.0, 2.0]);
        let mut b = a.clone();
        assert!(Shared::ptr_eq(&a, &b), "a copy shares");
        b.push(3.0);
        assert!(!Shared::ptr_eq(&a, &b), "changing one copies it");
        assert_eq!(*a, vec![1.0, 2.0], "the other keeps its value");
        assert_eq!(*b, vec![1.0, 2.0, 3.0]);
        for x in &mut b {
            *x *= 2.0;
        }
        assert_eq!(b.iter().sum::<f64>(), 12.0);
        assert_eq!(b.into_inner(), vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn compared_and_written_as_the_value_itself() {
        // Held once, a NaN is still not equal to itself, as before the value was shared.
        let nan = Shared::new(vec![f64::NAN]);
        assert_ne!(nan, nan.clone());
        assert_eq!(Shared::new(vec![1]), Shared::new(vec![1]));
        assert_eq!(serde_json::to_string(&Shared::new(vec![1, 2])).unwrap(), "[1,2]");
        let back: Shared<Vec<i32>> = serde_json::from_str("[3]").unwrap();
        assert_eq!(*back, vec![3]);
        assert_eq!(format!("{:?}", Shared::new(vec![1])), "[1]");
        let collected: Shared<Vec<i32>> = (1..=2).collect();
        assert_eq!(*collected, vec![1, 2]);
    }

    #[test]
    fn a_documents_copies_share_its_styles_and_variables() {
        let mut d = Document::new("t");
        let copy = d.clone();
        assert!(Shared::ptr_eq(&d.variables, &copy.variables) && Shared::ptr_eq(&d.styles, &copy.styles));
        d.variables.collections.clear();
        assert!(!Shared::ptr_eq(&d.variables, &copy.variables), "a change copies the document's own");
        assert!(Shared::ptr_eq(&d.styles, &copy.styles), "the rest stays shared");
        assert_eq!(d.variables, Shared::new(Variables::default()));
        // The file is written as before: the values themselves.
        let json = serde_json::to_value(&copy).unwrap();
        assert_eq!(json["variables"], serde_json::to_value(&*copy.variables).unwrap());
        assert_eq!(json["styles"], serde_json::json!([]));
    }
}
