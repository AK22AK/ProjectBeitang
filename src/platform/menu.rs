use gpui::{Action, Menu, MenuItem, OsAction, SystemMenuType};

pub fn build_app_menus<QuickAddAction, SearchAction, SettingsAction, QuitAction>(
    open_quick_add: QuickAddAction,
    open_search: SearchAction,
    open_settings: SettingsAction,
    quit: QuitAction,
) -> Vec<Menu>
where
    QuickAddAction: Action + Clone + 'static,
    SearchAction: Action + Clone + 'static,
    SettingsAction: Action + Clone + 'static,
    QuitAction: Action + Clone + 'static,
{
    #[cfg(target_os = "macos")]
    {
        return vec![
            Menu {
                name: "Robinne".into(),
                items: vec![
                    MenuItem::os_submenu("服务", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("设置...", open_settings.clone()),
                    MenuItem::separator(),
                    MenuItem::action("退出 Robinne", quit.clone()),
                ],
            },
            file_menu(open_quick_add.clone()),
            edit_menu(open_search.clone()),
        ];
    }

    #[cfg(not(target_os = "macos"))]
    {
        vec![
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("快速创建", open_quick_add.clone()),
                    MenuItem::action("设置", open_settings.clone()),
                    MenuItem::separator(),
                    MenuItem::action("退出", quit.clone()),
                ],
            },
            edit_menu(open_search),
        ]
    }
}

fn file_menu<QuickAddAction>(open_quick_add: QuickAddAction) -> Menu
where
    QuickAddAction: Action + Clone + 'static,
{
    Menu {
        name: "File".into(),
        items: vec![MenuItem::action("快速创建", open_quick_add)],
    }
}

fn edit_menu<SearchAction>(open_search: SearchAction) -> Menu
where
    SearchAction: Action + Clone + 'static,
{
    Menu {
        name: "Edit".into(),
        items: vec![
            MenuItem::os_action("撤销", gpui_component::input::Undo, OsAction::Undo),
            MenuItem::os_action("重做", gpui_component::input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("剪切", gpui_component::input::Cut, OsAction::Cut),
            MenuItem::os_action("复制", gpui_component::input::Copy, OsAction::Copy),
            MenuItem::os_action("粘贴", gpui_component::input::Paste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action(
                "全选",
                gpui_component::input::SelectAll,
                OsAction::SelectAll,
            ),
            MenuItem::separator(),
            MenuItem::action("搜索", open_search),
        ],
    }
}
