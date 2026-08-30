use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub trait EntityId {
    fn raw(&self) -> Uuid;
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        #[allow(clippy::new_without_default)]
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl EntityId for $name {
            fn raw(&self) -> Uuid {
                self.0
            }
        }
    };
}

id_type!(AccountId);
id_type!(CharacterId);
id_type!(SessionId);
id_type!(PlayerId);
id_type!(ShipInstanceId);
id_type!(ShipDefinitionId);
id_type!(ItemInstanceId);
id_type!(ItemDefinitionId);
id_type!(RegionId);
id_type!(ZoneId);
id_type!(ResourceNodeId);
id_type!(NpcInstanceId);
id_type!(RecipeId);
id_type!(MarketOrderId);
id_type!(TransactionId);

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde::{Deserialize, Serialize};

    use super::*;

    struct UuidString;

    fn unsupported<T>() -> Result<T, serde::de::value::Error> {
        Err(<serde::de::value::Error as serde::ser::Error>::custom(
            "unsupported",
        ))
    }

    impl serde::Serializer for UuidString {
        type Ok = String;
        type Error = serde::de::value::Error;
        type SerializeSeq = serde::ser::Impossible<String, Self::Error>;
        type SerializeTuple = serde::ser::Impossible<String, Self::Error>;
        type SerializeTupleStruct = serde::ser::Impossible<String, Self::Error>;
        type SerializeTupleVariant = serde::ser::Impossible<String, Self::Error>;
        type SerializeMap = serde::ser::Impossible<String, Self::Error>;
        type SerializeStruct = serde::ser::Impossible<String, Self::Error>;
        type SerializeStructVariant = serde::ser::Impossible<String, Self::Error>;

        fn serialize_bool(self, _: bool) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_i8(self, _: i8) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_i16(self, _: i16) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_i32(self, _: i32) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_i64(self, _: i64) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_u8(self, _: u8) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_u16(self, _: u16) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_u32(self, _: u32) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_u64(self, _: u64) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_f32(self, _: f32) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_f64(self, _: f64) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_char(self, _: char) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_str(self, value: &str) -> Result<String, Self::Error> {
            Ok(value.to_owned())
        }

        fn serialize_bytes(self, _: &[u8]) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_none(self) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_some<T>(self, _: &T) -> Result<String, Self::Error>
        where
            T: ?Sized + Serialize,
        {
            unsupported()
        }

        fn serialize_unit(self) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_unit_struct(self, _: &'static str) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_unit_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
        ) -> Result<String, Self::Error> {
            unsupported()
        }

        fn serialize_newtype_struct<T>(
            self,
            _: &'static str,
            value: &T,
        ) -> Result<String, Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(self)
        }

        fn serialize_newtype_variant<T>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: &T,
        ) -> Result<String, Self::Error>
        where
            T: ?Sized + Serialize,
        {
            unsupported()
        }

        fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
            unsupported()
        }

        fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
            unsupported()
        }

        fn serialize_tuple_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleStruct, Self::Error> {
            unsupported()
        }

        fn serialize_tuple_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleVariant, Self::Error> {
            unsupported()
        }

        fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
            unsupported()
        }

        fn serialize_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStruct, Self::Error> {
            unsupported()
        }

        fn serialize_struct_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStructVariant, Self::Error> {
            unsupported()
        }
    }

    #[test]
    fn new_generates_distinct_ids() {
        assert_ne!(AccountId::new(), AccountId::new());
    }

    #[test]
    fn from_uuid_preserves_value() {
        let uuid = Uuid::new_v4();
        assert_eq!(CharacterId::from(uuid), CharacterId(uuid));
    }

    #[test]
    fn entity_id_returns_raw_uuid() {
        let uuid = Uuid::new_v4();
        assert_eq!(ShipInstanceId::from(uuid).raw(), uuid);
    }

    #[test]
    fn serde_roundtrip_preserves_value() {
        let id = PlayerId::new();
        let serialized = id.serialize(UuidString).unwrap();
        assert_eq!(serialized, id.0.to_string());

        let deserializer =
            serde::de::value::StrDeserializer::<serde::de::value::Error>::new(serialized.as_str());
        let decoded = PlayerId::deserialize(deserializer).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn hash_set_deduplicates_ids() {
        let id = ShipDefinitionId::new();
        let mut ids = HashSet::new();
        ids.insert(id);
        ids.insert(id);
        assert_eq!(ids.len(), 1);
    }
}
