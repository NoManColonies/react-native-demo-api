use tokio::task::JoinHandle;

/// a wrapper function with the same usage as `tokio::spawn()` or `tokio::task::spawn()` but with
/// extra functionalities. This function will require a name for the tokio::task however the name
/// will only serve a meaningful functionality when `tokio_unstable` configuration is enabled
/// otherwise this is will do exactly the same thing as `tokio::spawn(..)`
pub fn spawn_with_name<T, I>(future: T, _name: I) -> JoinHandle<T::Output>
where
    T: std::future::Future + Send + 'static,
    T::Output: Send + 'static,
    I: AsRef<str>,
{
    #[cfg(tokio_unstable)]
    return tokio::task::Builder::new()
        .name(_name.as_ref())
        .spawn(future);
    #[cfg(not(tokio_unstable))]
    return tokio::spawn(future);
}
