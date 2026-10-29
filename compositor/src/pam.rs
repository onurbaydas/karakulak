//! Checks a password with PAM. Only the three calls we need, declared by hand
//! (Linux-PAM's ABI) instead of generating bindings at build time.

use std::ffi::{CString, c_char, c_int, c_void};

const PAM_SUCCESS: c_int = 0;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_CONV_ERR: c_int = 19;

#[repr(C)]
struct Message {
    style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct Response {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct Conversation {
    conv: extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    appdata: *mut c_void,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conv: *const Conversation, handle: *mut *mut c_void) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_acct_mgmt(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
}

/// Answers every password prompt with the password in `appdata`.
extern "C" fn answer(count: c_int, messages: *mut *const Message, responses: *mut *mut Response, appdata: *mut c_void) -> c_int {
    let Ok(count) = usize::try_from(count) else {
        return PAM_CONV_ERR;
    };
    // PAM frees the array and the strings, so they come from malloc.
    let replies = unsafe { libc::calloc(count, std::mem::size_of::<Response>()) } as *mut Response;
    if replies.is_null() {
        return PAM_CONV_ERR;
    }
    for i in 0..count {
        let style = unsafe { (**messages.add(i)).style };
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            unsafe { (*replies.add(i)).resp = libc::strdup(appdata as *const c_char) };
        }
    }
    unsafe { *responses = replies };
    PAM_SUCCESS
}

/// True if `password` is right for `user` under PAM `service`.
pub fn authenticate(service: &str, user: &str, password: &str) -> bool {
    let (Ok(service), Ok(user), Ok(password)) = (CString::new(service), CString::new(user), CString::new(password))
    else {
        return false;
    };
    let conversation = Conversation {
        conv: answer,
        appdata: password.as_ptr() as *mut c_void,
    };
    let mut handle = std::ptr::null_mut();
    unsafe {
        if pam_start(service.as_ptr(), user.as_ptr(), &conversation, &mut handle) != PAM_SUCCESS {
            return false;
        }
        let mut status = pam_authenticate(handle, 0);
        if status == PAM_SUCCESS {
            status = pam_acct_mgmt(handle, 0);
        }
        pam_end(handle, status);
        status == PAM_SUCCESS
    }
}

#[cfg(test)]
mod tests {
    /// Needs a user `karakulak-test` with password `dogru` (see CI or a
    /// throwaway container); skipped otherwise.
    #[test]
    fn checks_password() {
        if std::env::var_os("KARAKULAK_PAM_TEST").is_none() {
            return;
        }
        assert!(super::authenticate("login", "karakulak-test", "dogru"));
        assert!(!super::authenticate("login", "karakulak-test", "yanlis"));
    }
}
