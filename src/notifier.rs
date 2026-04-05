use crate::models::Record;

pub struct Notifier;

impl Notifier {
    pub fn send_reminder(task: &Record) -> Result<(), String> {
        let title = "Beitang 提醒";
        let body = format!("时间到了！\n{}", task.content);

        let mut n = notify_rust::Notification::new();
        n.summary(title)
            .body(&body)
            .appname("Beitang")
            .timeout(notify_rust::Timeout::Milliseconds(10000));

        #[cfg(target_os = "macos")]
        n.sound_name("Ping");

        match n.show() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to send notification: {}", e)),
        }
    }
}
