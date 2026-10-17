use boot_core::*;
use boot_windows::store::validate_path_components;
#[test]
fn reparse_storage_path_is_rejected() {
    assert_eq!(
        validate_path_components(&[false, true, false])
            .unwrap_err()
            .code,
        ErrorCode::StorageUnsafe
    );
}
