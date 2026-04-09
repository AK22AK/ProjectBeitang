import Cocoa

// 从命令行参数获取任务名和默认日期
let taskName = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ""
let defaultDateStr = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : ""

// 创建 NSApplication 实例（无 Dock 图标模式）
let app = NSApplication.shared
app.setActivationPolicy(.accessory)

// 创建日期选择器
let datePicker = NSDatePicker(frame: NSRect(x: 0, y: 0, width: 300, height: 200))
datePicker.datePickerStyle = .clockAndCalendar
datePicker.datePickerElements = [.yearMonthDay, .hourMinute]

// 设置默认日期
if !defaultDateStr.isEmpty {
    let formatter = DateFormatter()
    formatter.dateFormat = "yyyy-MM-dd HH:mm"
    if let date = formatter.date(from: defaultDateStr) {
        datePicker.dateValue = date
    }
} else {
    datePicker.dateValue = Date()
}

// 设置最小日期为当前时间
datePicker.minDate = Date()

// 创建弹窗
let alert = NSAlert()
alert.messageText = "Robinne 提醒设置"
alert.informativeText = taskName.isEmpty ? "请选择提醒日期和时间" : "为任务「\(taskName)」设置提醒时间"
alert.addButton(withTitle: "设定")
alert.addButton(withTitle: "取消")
alert.accessoryView = datePicker
alert.window.level = .floating

// 激活应用使弹窗显示在最前
app.activate(ignoringOtherApps: true)

// 运行弹窗
let response = alert.runModal()

if response == .alertFirstButtonReturn {
    let formatter = DateFormatter()
    formatter.dateFormat = "yyyy-MM-dd HH:mm"
    print(formatter.string(from: datePicker.dateValue))
} else {
    // 用户点了取消
    exit(1)
}
