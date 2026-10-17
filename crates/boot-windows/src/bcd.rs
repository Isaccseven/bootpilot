use boot_core::*;
#[cfg(windows)]
use uuid::Uuid;
#[cfg(windows)]
use wmi::{IWbemClassWrapper, Variant, WMIConnection};
pub fn bcd_return_is_success(value: &serde_json::Value) -> bool {
    value == &serde_json::Value::Bool(true)
}
pub fn bitlocker_return_is_success(value: &serde_json::Value) -> bool {
    value.as_u64() == Some(0) && !value.is_boolean()
}
#[cfg(windows)]
fn put_input(input: &IWbemClassWrapper, key: &str, value: Variant) -> wmi::WMIResult<()> {
    if matches!(&value,Variant::String(s) if s.is_empty()) {
        // windows BSTR::from("") is a null BSTR. This provider rejects that with
        // WBEM_E_OUT_OF_MEMORY. SysAllocStringLen allocates a genuine zero-length BSTR.
        unsafe {
            use windows::{
                Win32::{Foundation::SysAllocStringLen, System::Variant::VARIANT},
                core::{BSTR, PCWSTR},
            };
            let raw = SysAllocStringLen(Some(&[])).into_raw();
            if raw.is_null() {
                return Err(wmi::WMIError::from(windows::core::Error::from_hresult(
                    windows::core::HRESULT(0x8007000eu32 as i32),
                )));
            }
            let value = VARIANT::from(BSTR::from_raw(raw));
            input
                .inner
                .Put(PCWSTR(crate::native::wide(key).as_ptr()), 0, &value, 0)?;
            Ok(())
        }
    } else {
        input.put_property(key, value)
    }
}
#[cfg(windows)]
pub(crate) fn call(
    connection: &WMIConnection,
    class: &str,
    path: &str,
    method: &str,
    args: &[(&str, Variant)],
) -> BootResult<IWbemClassWrapper> {
    let context = |stage: &str, e: wmi::WMIError| {
        let mut e = crate::native::wmi_error(e);
        e.details["stage"] = serde_json::json!(format!("{class}.{method}:{stage}"));
        e
    };
    let signature = connection
        .get_object(class)
        .map_err(|e| context("class", e))?
        .get_method(method)
        .map_err(|e| context("signature", e))?;
    let input = if let Some(signature) = signature {
        let input = signature
            .spawn_instance()
            .map_err(|e| context("spawn", e))?;
        for (key, value) in args {
            put_input(&input, key, value.clone()).map_err(|e| context(key, e))?;
        }
        Some(input)
    } else {
        None
    };
    connection
        .exec_method(path, method, input.as_ref())
        .map_err(|e| context("invoke", e))?
        .ok_or_else(|| crate::error(ErrorCode::VerificationFailed, "WMI-Antwort fehlt"))
}
#[cfg(windows)]
fn checked(output: IWbemClassWrapper) -> BootResult<IWbemClassWrapper> {
    if output
        .get_property("ReturnValue")
        .map_err(crate::native::wmi_error)?
        != Variant::Bool(true)
    {
        return Err(crate::error(
            ErrorCode::VerificationFailed,
            "BCD-Provider hat die Abfrage abgelehnt",
        ));
    }
    Ok(output)
}
#[cfg(windows)]
fn object(output: &IWbemClassWrapper, key: &str) -> BootResult<IWbemClassWrapper> {
    match output.get_property(key).map_err(crate::native::wmi_error)? {
        Variant::Object(v) => Ok(v),
        _ => Err(crate::error(
            ErrorCode::VerificationFailed,
            "WMI-Objekt fehlt",
        )),
    }
}
#[cfg(windows)]
fn open(connection: &WMIConnection, store: &str, id: Uuid) -> BootResult<IWbemClassWrapper> {
    let object = object(
        &checked(call(
            connection,
            "BcdStore",
            store,
            "OpenObject",
            &[("Id", Variant::String(format!("{{{id}}}")))],
        )?)?,
        "Object",
    )?;
    match object
        .get_property("Id")
        .map_err(crate::native::wmi_error)?
    {
        Variant::String(value) if Uuid::parse_str(&value).ok() == Some(id) => Ok(object),
        _ => Err(crate::error(
            ErrorCode::VerificationFailed,
            "BCD-Objektidentität stimmt nicht überein",
        )),
    }
}
#[cfg(windows)]
fn instance_path(object: &IWbemClassWrapper, class: &str) -> BootResult<String> {
    // Method outputs are embedded objects and may have __PATH=null. Build only
    // the two fixed system-store key forms, after validating returned keys.
    let key = if class == "BcdStore" {
        "FilePath"
    } else {
        "StoreFilePath"
    };
    if object.get_property(key).map_err(crate::native::wmi_error)? != Variant::String(String::new())
    {
        return Err(crate::error(
            ErrorCode::VerificationFailed,
            "BCD-Objekt gehört nicht zum System-Store",
        )
        .with_details(serde_json::json!({"field":key})));
    }
    if class == "BcdStore" {
        return Ok(r#"BcdStore.FilePath="""#.into());
    }
    let Variant::String(id) = object
        .get_property("Id")
        .map_err(crate::native::wmi_error)?
    else {
        return Err(crate::error(
            ErrorCode::VerificationFailed,
            "BCD-Objekt-ID fehlt",
        ));
    };
    let id = Uuid::parse_str(&id).map_err(crate::io_error)?;
    Ok(format!(r#"BcdObject.Id="{{{id}}}",StoreFilePath="""#))
}
#[cfg(windows)]
fn element(
    connection: &WMIConnection,
    object: &IWbemClassWrapper,
    kind: u32,
) -> BootResult<IWbemClassWrapper> {
    let path = instance_path(object, "BcdObject")?;
    self::object(
        &checked(call(
            connection,
            "BcdObject",
            &path,
            "GetElement",
            &[("Type", Variant::UI4(kind))],
        )?)?,
        "Element",
    )
}
#[cfg(windows)]
pub(crate) fn collect(snapshot: &mut Snapshot) -> BootResult<()> {
    let _backup = crate::native::enable_privilege("SeBackupPrivilege")?;
    let _restore = crate::native::enable_privilege("SeRestorePrivilege")?;
    let connection =
        WMIConnection::with_namespace_path("ROOT\\WMI").map_err(crate::native::wmi_error)?;
    let store_object = object(
        &checked(call(
            &connection,
            "BcdStore",
            "BcdStore",
            "OpenStore",
            &[("File", Variant::String(String::new()))],
        )?)?,
        "Store",
    )?;
    let store = instance_path(&store_object, "BcdStore")?;
    let manager = open(
        &connection,
        &store,
        Uuid::parse_str("a5a30fa2-3d06-4e9f-b5f4-a01df9d1fcba").unwrap(),
    )?;
    if manager
        .get_property("Type")
        .map_err(crate::native::wmi_error)?
        != Variant::UI4(0x10100001)
    {
        return Err(crate::error(
            ErrorCode::VerificationFailed,
            "BCD-Firmwaremanager hat einen unerwarteten Objekttyp",
        ));
    }
    match element(&connection, &manager, 0x24000001)?
        .get_property("Ids")
        .map_err(crate::native::wmi_error)?
    {
        Variant::Array(ids) => {
            for id in ids {
                let Variant::String(id) = id else {
                    return Err(crate::error(
                        ErrorCode::VerificationFailed,
                        "BCD-ID ist keine Zeichenkette",
                    ));
                };
                let id = Uuid::parse_str(&id).map_err(crate::io_error)?;
                if snapshot.firmware_order.contains(&id) {
                    return Err(crate::error(
                        ErrorCode::VerificationFailed,
                        "Doppelte BCD-ID",
                    ));
                }
                snapshot.firmware_order.push(id);
            }
        }
        _ => {
            return Err(crate::error(
                ErrorCode::VerificationFailed,
                "BCD-Reihenfolge ist kein Array",
            ));
        }
    }
    let mut errors = Vec::new();
    for id in snapshot.firmware_order.clone() {
        match open(&connection, &store, id) {
            Ok(obj) => {
                let title = element(&connection, &obj, 0x12000004).and_then(|v| {
                    match v.get_property("String").map_err(crate::native::wmi_error)? {
                        Variant::String(s) => Ok(s),
                        _ => Err(crate::error(
                            ErrorCode::VerificationFailed,
                            "BCD-Beschreibung fehlt",
                        )),
                    }
                });
                let title = match title {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(e);
                        format!("BCD {id}")
                    }
                };
                // Application paths are optional for firmware hardware entries.
                let loader_path = element(&connection, &obj, 0x12000002)
                    .ok()
                    .and_then(|v| match v.get_property("String").ok()? {
                        Variant::String(s) => Some(s),
                        _ => None,
                    });
                snapshot.firmware_entries.push(FirmwareEntry {
                    id,
                    title,
                    loader_path,
                });
            }
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(crate::error(
            ErrorCode::IncompleteSnapshot,
            "Einzelne BCD-Einträge sind nicht lesbar",
        )
        .with_details(serde_json::json!({"errors":errors})))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn embedded_objects_use_validated_keys_without_path_metadata() {
        let con = WMIConnection::with_namespace_path("ROOT\\WMI").unwrap();
        let store = con
            .get_object("BcdStore")
            .unwrap()
            .spawn_instance()
            .unwrap();
        put_input(&store, "FilePath", Variant::String(String::new())).unwrap();
        assert_eq!(store.get_property("__PATH").unwrap(), Variant::Null);
        assert_eq!(
            instance_path(&store, "BcdStore").unwrap(),
            r#"BcdStore.FilePath="""#
        );
        put_input(&store, "FilePath", Variant::String("C:\\foreign".into())).unwrap();
        assert_eq!(
            instance_path(&store, "BcdStore").unwrap_err().code,
            ErrorCode::VerificationFailed
        );
        let object = con
            .get_object("BcdObject")
            .unwrap()
            .spawn_instance()
            .unwrap();
        put_input(&object, "StoreFilePath", Variant::String(String::new())).unwrap();
        put_input(
            &object,
            "Id",
            Variant::String("{00112233-4455-6677-8899-aabbccddeeff}".into()),
        )
        .unwrap();
        assert_eq!(
            instance_path(&object, "BcdObject").unwrap(),
            r#"BcdObject.Id="{00112233-4455-6677-8899-aabbccddeeff}",StoreFilePath="""#
        );
    }
    #[test]
    fn empty_system_store_name_is_a_real_empty_bstr() {
        let con = WMIConnection::with_namespace_path("ROOT\\WMI").unwrap();
        let input = con
            .get_object("BcdStore")
            .unwrap()
            .get_method("OpenStore")
            .unwrap()
            .unwrap()
            .spawn_instance()
            .unwrap();
        put_input(&input, "File", Variant::String(String::new())).unwrap();
        assert_eq!(
            input.get_property("File").unwrap(),
            Variant::String(String::new())
        );
    }
    #[test]
    fn wmi_marshals_bcd_string_arrays_without_invoking_mutation() {
        let con = WMIConnection::with_namespace_path("ROOT\\WMI").unwrap();
        let signature = con
            .get_object("BcdObject")
            .unwrap()
            .get_method("SetObjectListElement")
            .unwrap()
            .unwrap()
            .spawn_instance()
            .unwrap();
        let ids = Variant::Array(vec![Variant::String(
            "{00112233-4455-6677-8899-aabbccddeeff}".into(),
        )]);
        signature
            .put_property("Type", Variant::UI4(0x24000001))
            .unwrap();
        signature.put_property("Ids", ids.clone()).unwrap();
        assert_eq!(signature.get_property("Ids").unwrap(), ids);
    }
}
