use sentry::{capture_message, Level};

pub fn capture_warning<T>(msg: T)
where
    T: AsRef<str>,
{
    capture_message(msg.as_ref(), Level::Warning);
}

pub fn capture_error<T>(msg: T)
where
    T: AsRef<str>,
{
    capture_message(msg.as_ref(), Level::Error);
}

pub fn capture_fatal<T>(msg: T)
where
    T: AsRef<str>,
{
    capture_message(msg.as_ref(), Level::Fatal);
}
