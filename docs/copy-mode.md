# Vim-style scrollback copy mode

Zellij 的 `scroll` 模式可以使用 Vim 风格的光标移动和选择来复制终端回滚内容。

先按 `Ctrl-s` 进入 `scroll` 模式，再按 `v` 进入字符选择。移动光标后按 `y` 复制；`V` 可以从当前行开始进行行选择。第一次按 `Esc` 取消选择，第二次按 `Esc` 退出 copy mode。

默认键位如下：

| 键 | 作用 |
| --- | --- |
| `v` | 进入或切换字符选择 |
| `V` | 行选择，从当前行开始 |
| `h` / `j` / `k` / `l` | 左、下、上、右移动 |
| `w` | 跳到下一个单词开头 |
| `e` | 跳到当前或下一个单词结尾 |
| `b` | 跳到上一个单词开头 |
| `0` | 跳到行首 |
| `$` | 跳到行尾 |
| `y` | 复制当前选择并退出 |
| `Esc` | 取消选择，或退出 copy mode |
| `Ctrl-f` / `Ctrl-b` | 向下 / 向上翻页 |
| `d` / `u` | 向下 / 向上翻半页 |

如果配置文件使用了 `clear-defaults=true`，内置默认键位会被全部清空，需要在自己的 `scroll` 块中重新绑定。例如：

```kdl
keybinds clear-defaults=true {
    scroll {
        bind "Ctrl s" { SwitchToMode "scroll"; }
        bind "v" { ToggleCopyMode; }
        bind "V" { CopyModeLineSelect; }
        bind "h" "Left" { CopyModeMove "left"; }
        bind "j" "Down" { CopyModeMove "down"; }
        bind "k" "Up" { CopyModeMove "up"; }
        bind "l" "Right" { CopyModeMove "right"; }
        bind "w" { CopyModeWordStart; }
        bind "e" { CopyModeWordEnd; }
        bind "b" { CopyModeWordBack; }
        bind "0" { CopyModeLineStart; }
        bind "$" { CopyModeLineEnd; }
        bind "y" { CopyModeYank; }
        bind "Esc" { CopyModeCancel; }
    }
}
```

这些绑定只作用于 `scroll` 模式，不会改变 `search` 模式中的 `j/k`、`w` 等键位。修改配置后，需要新建 session，或者重启使用该配置的 server；已经运行的 session 不会自动重新读取配置。
