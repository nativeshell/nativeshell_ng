use std::ffi::c_void;

use core_foundation::{
    base::{CFGetTypeID, CFTypeRef},
    boolean::{CFBooleanGetTypeID, CFBooleanRef},
    number::{
        kCFNumberFloat64Type, kCFNumberSInt64Type, CFBooleanGetValue, CFNumberGetValue, CFNumberRef,
    },
};
use objc::{
    class, msg_send,
    rc::{autoreleasepool, StrongPtr},
    runtime, sel, sel_impl,
};

use crate::{
    platform::sys::{cocoa::nil, to_nsdata, to_nsstring},
    TryFromError, Value,
};

use super::sys::{
    cocoa::{id, NSArray, NSDictionary},
    from_nsdata, from_nsstring,
};

/// Trait for converting Value from and to Objective C objects.
pub trait ValueObjcConversion: Sized {
    fn to_objc(&self) -> Result<StrongPtr, TryFromError>;
    fn from_objc(objc: *mut runtime::Object) -> Result<Self, TryFromError>;
}

impl ValueObjcConversion for Value {
    fn to_objc(&self) -> Result<StrongPtr, TryFromError> {
        autoreleasepool(|| unsafe { _value_to_objc(self).map(|f| StrongPtr::retain(f)) })
    }

    fn from_objc(obj: *mut runtime::Object) -> Result<Self, TryFromError> {
        autoreleasepool(|| unsafe { _value_from_objc(obj) })
    }
}

//
//
//

extern "C" {
    pub fn CFNumberIsFloatType(number: CFNumberRef) -> bool;
}

unsafe fn _value_from_objc(obj: id) -> Result<Value, TryFromError> {
    if obj.is_null() || obj == msg_send![class!(NSNull), null] {
        Ok(Value::Null)
    } else if msg_send![obj, isKindOfClass: class!(NSNumber)] {
        let cf = obj as CFNumberRef;
        if CFGetTypeID(cf as CFTypeRef) == CFBooleanGetTypeID() {
            Ok(Value::Bool(CFBooleanGetValue(cf as CFBooleanRef)))
        } else if CFNumberIsFloatType(cf) {
            let mut value = 0f64;
            CFNumberGetValue(
                cf as CFNumberRef,
                kCFNumberFloat64Type,
                &mut value as *mut _ as *mut c_void,
            );
            Ok(Value::F64(value))
        } else {
            let mut value = 0i64;
            CFNumberGetValue(
                cf as CFNumberRef,
                kCFNumberSInt64Type,
                &mut value as *mut _ as *mut c_void,
            );
            Ok(Value::I64(value))
        }
    } else if msg_send![obj, isKindOfClass: class!(NSString)] {
        Ok(Value::String(from_nsstring(obj)))
    } else if msg_send![obj, isKindOfClass: class!(NSData)] {
        Ok(Value::U8List(from_nsdata(obj)))
    } else if msg_send![obj, isKindOfClass: class!(NSArray)] {
        let len = NSArray::count(obj);
        let mut res = Vec::<Value>::with_capacity(len as usize);
        for i in 0..len {
            let item = NSArray::objectAtIndex(obj, i);
            res.push(_value_from_objc(item)?);
        }
        Ok(Value::List(res))
    } else if msg_send![obj, isKindOfClass: class!(NSDictionary)] {
        let mut entries = Vec::<(Value, Value)>::new();
        let keys = NSDictionary::keyEnumerator(obj);
        loop {
            let key: id = msg_send![keys, nextObject];
            if key.is_null() {
                break;
            } else {
                let value = NSDictionary::valueForKey_(obj, key);
                entries.push((_value_from_objc(key)?, _value_from_objc(value)?));
            }
        }

        Ok(entries.into())
    } else {
        let class_name: id = msg_send![obj, className];
        Err(TryFromError::OtherError(format!(
            "Unable to convert {} to Value",
            from_nsstring(class_name)
        )))
    }
}

unsafe fn _value_to_objc(value: &Value) -> Result<id, TryFromError> {
    unsafe fn fix_null(v: id) -> id {
        if v.is_null() {
            msg_send![class!(NSNull), null]
        } else {
            v
        }
    }
    unsafe fn transform_slice<T>(s: &[T]) -> &[u8] {
        std::slice::from_raw_parts(s.as_ptr() as *const u8, s.len() * std::mem::size_of::<T>())
    }

    match value {
        Value::Null => Ok(nil),
        Value::Bool(v) => Ok(msg_send![class!(NSNumber), numberWithBool: *v]),
        Value::I64(v) => Ok(msg_send![class!(NSNumber), numberWithLongLong: *v]),
        Value::F64(v) => Ok(msg_send![class!(NSNumber), numberWithDouble: *v]),
        Value::String(s) => Ok(to_nsstring(s).autorelease()),
        Value::U8List(d) => Ok(to_nsdata(d).autorelease()),
        Value::I8List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::U16List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::I16List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::U32List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::I32List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::I64List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::F32List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::F64List(d) => Ok(to_nsdata(transform_slice(d)).autorelease()),
        Value::List(items) => {
            let res = items
                .iter()
                .map(|v| _value_to_objc(v).map(|v| fix_null(v)))
                .collect::<Result<Vec<_>, TryFromError>>()?;
            Ok(NSArray::arrayWithObjects(nil, &res))
        }
        Value::Map(items) => {
            let mut keys = Vec::<id>::with_capacity(items.len());
            let mut objects = Vec::<id>::with_capacity(items.len());
            for item in items.iter() {
                keys.push(fix_null(_value_to_objc(&item.0)?));
                objects.push(fix_null(_value_to_objc(&item.1)?));
            }
            Ok(NSDictionary::dictionaryWithObjects_forKeys_(
                nil,
                NSArray::arrayWithObjects(nil, &objects),
                NSArray::arrayWithObjects(nil, &keys),
            ))
        }
        other => Err(TryFromError::OtherError(format!(
            "Unable to convert {:?} to Objc",
            other,
        ))),
    }
}

#[cfg(test)]
mod test {
    use objc::{class, msg_send, sel, sel_impl};

    use crate::{
        platform::{
            sys::{
                cocoa::{nil, NSArray, NSDictionary},
                to_nsdata, to_nsstring,
            },
            value::ValueObjcConversion,
        },
        Value,
    };

    #[test]
    #[cfg(target_endian = "little")]
    fn test_coerce_data() {
        let v: Value = vec![1i8, 2i8, 3i8].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 2u8, 3u8,])]
        });

        let v: Value = vec![1i8, 2i8, 3i8].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 2u8, 3u8,])]
        });

        unsafe fn transform_slice<T>(s: &[T]) -> &[u8] {
            std::slice::from_raw_parts(s.as_ptr() as *const u8, s.len() * std::mem::size_of::<T>())
        }

        let v: Value = vec![1f32, 2f32].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(transform_slice(&[1f32, 2f32]))]
        });

        let v: Value = vec![1f64, 2f64].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(transform_slice(&[1f64, 2f64]))]
        });
    }

    #[test]
    #[cfg(target_endian = "little")]
    fn test_coerce_data_l() {
        let v: Value = vec![1u16, 2u16, 3u16].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 0u8, 2u8, 0u8, 3u8, 0u8])]
        });

        let v: Value = vec![1i16, 2i16, 3i16].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 0u8, 2u8, 0u8, 3u8, 0u8])]
        });

        let v: Value = vec![1u32].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 0u8, 0u8, 0u8])]
        });

        let v: Value = vec![1i32].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 0u8, 0u8, 0u8])]
        });

        let v: Value = vec![1i64].into();
        assert!(unsafe {
            msg_send![*v.to_objc().unwrap(), isEqual: *to_nsdata(&[1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8])]
        });
    }

    #[test]
    fn tests() {
        let object1 = unsafe {
            NSDictionary::dictionaryWithObject_forKey_(
                nil,
                NSArray::arrayWithObjects(
                    nil,
                    &[
                        *to_nsstring("Obj1"),
                        msg_send![class!(NSNumber), numberWithBool: false],
                        msg_send![class!(NSNumber), numberWithBool: true],
                        msg_send![class!(NSNumber), numberWithInt: 5],
                        msg_send![class!(NSNumber), numberWithFloat: 10.0f32],
                        msg_send![class!(NSNumber), numberWithDouble: 15.0f64],
                        *to_nsdata(&[1, 2, 3]),
                        msg_send![class!(NSNull), null],
                    ],
                ),
                *to_nsstring("Key"),
            )
        };
        let value = Value::from_objc(object1).unwrap();
        let our_value: Value = vec![(
            "Key".into(),
            vec![
                Value::String("Obj1".into()),
                false.into(),
                true.into(),
                5i64.into(),
                10f64.into(),
                15f64.into(),
                vec![1u8, 2u8, 3u8].into(),
                Value::Null,
            ]
            .into(),
        )]
        .into();

        assert_eq!(value, our_value);

        let objc = our_value.to_objc().unwrap();

        let equals: bool = unsafe { msg_send![*objc, isEqual: object1] };
        assert!(equals);
    }
}
