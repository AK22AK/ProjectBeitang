use crate::models::Record;

pub fn send_reminder(task: &Record) -> Result<(), String> {
    let title = "Robinne 提醒";
    let body = format!("时间到了！\n{}", task.content);

    let mut notification = notify_rust::Notification::new();
    notification
        .summary(title)
        .body(&body)
        .appname("Robinne")
        .timeout(notify_rust::Timeout::Milliseconds(10000));

    #[cfg(target_os = "macos")]
    notification.sound_name("Ping");

    notification
        .show()
        .map(|_| ())
        .map_err(|error| format!("Failed to send notification: {error}"))
}
