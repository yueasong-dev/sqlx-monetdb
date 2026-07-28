use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use sqlx_core::decode::Decode;
use sqlx_core::encode::{Encode, IsNull};
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

// docs/DEVELOPMENT.md §4.5 text formats:
//   date:      YYYY-MM-DD
//   time:      HH:MM:SS[.ffffff]
//   timestamp: YYYY-MM-DD HH:MM:SS[.ffffff]

impl Type<Monet> for NaiveDate {
    fn type_info() -> MonetTypeInfo {
        MonetTypeInfo::new("date")
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        compatible_with_names(ty, &["date"])
    }
}

impl<'r> Decode<'r, Monet> for NaiveDate {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        Ok(NaiveDate::parse_from_str(value.text()?, "%Y-%m-%d")?)
    }
}

impl<'q> Encode<'q, Monet> for NaiveDate {
    fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
        // Explicit `DATE '...'` cast prefix (matching pymonetdb's
        // monetize.py convention) rather than a bare string literal, to
        // avoid relying on implicit cast behavior in every query context.
        buf.extend_from_slice(format!("DATE '{}'", self.format("%Y-%m-%d")).as_bytes());
        Ok(IsNull::No)
    }
}

impl Type<Monet> for NaiveTime {
    fn type_info() -> MonetTypeInfo {
        MonetTypeInfo::new("time")
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        compatible_with_names(ty, &["time"])
    }
}

impl<'r> Decode<'r, Monet> for NaiveTime {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        Ok(NaiveTime::parse_from_str(value.text()?, "%H:%M:%S%.f")?)
    }
}

impl<'q> Encode<'q, Monet> for NaiveTime {
    fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
        buf.extend_from_slice(format!("TIME '{}'", self.format("%H:%M:%S%.f")).as_bytes());
        Ok(IsNull::No)
    }
}

impl Type<Monet> for NaiveDateTime {
    fn type_info() -> MonetTypeInfo {
        MonetTypeInfo::new("timestamp")
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        compatible_with_names(ty, &["timestamp"])
    }
}

impl<'r> Decode<'r, Monet> for NaiveDateTime {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        Ok(NaiveDateTime::parse_from_str(
            value.text()?,
            "%Y-%m-%d %H:%M:%S%.f",
        )?)
    }
}

impl<'q> Encode<'q, Monet> for NaiveDateTime {
    fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
        buf.extend_from_slice(
            format!("TIMESTAMP '{}'", self.format("%Y-%m-%d %H:%M:%S%.f")).as_bytes(),
        );
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_date() {
        let ty = MonetTypeInfo::new("date");
        let decoded = NaiveDate::decode(MonetValueRef::new("2026-07-28", &ty)).unwrap();
        assert_eq!(decoded, NaiveDate::from_ymd_opt(2026, 7, 28).unwrap());
    }

    #[test]
    fn decodes_time_with_fractional_seconds() {
        let ty = MonetTypeInfo::new("time");
        let decoded = NaiveTime::decode(MonetValueRef::new("13:45:30.500000", &ty)).unwrap();
        assert_eq!(
            decoded,
            NaiveTime::from_hms_micro_opt(13, 45, 30, 500_000).unwrap()
        );
    }

    #[test]
    fn decodes_time_without_fractional_seconds() {
        let ty = MonetTypeInfo::new("time");
        let decoded = NaiveTime::decode(MonetValueRef::new("13:45:30", &ty)).unwrap();
        assert_eq!(decoded, NaiveTime::from_hms_opt(13, 45, 30).unwrap());
    }

    #[test]
    fn decodes_timestamp() {
        let ty = MonetTypeInfo::new("timestamp");
        let decoded =
            NaiveDateTime::decode(MonetValueRef::new("2026-07-28 13:45:30.5", &ty)).unwrap();
        assert_eq!(
            decoded,
            NaiveDate::from_ymd_opt(2026, 7, 28)
                .unwrap()
                .and_hms_micro_opt(13, 45, 30, 500_000)
                .unwrap()
        );
    }

    #[test]
    fn rejects_malformed_date() {
        let ty = MonetTypeInfo::new("date");
        assert!(NaiveDate::decode(MonetValueRef::new("not-a-date", &ty)).is_err());
    }
}
