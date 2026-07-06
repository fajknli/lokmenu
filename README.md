# lok

wayland 菜单(配合脚本交互使用)


需要支持 `wlr-layer-shell`、`text-input-v3` 的 Wayland 合成器，可选支持 `fractional-scale` / `viewporter`。

## 安装

```bash
cargo install lokmenu

git clone <this repo>
```

## 参数

| 短选项 | 长选项 | 说明 | 默认值 |
|--------|--------|------|--------|
| `-p` | `--prompt` | 提示符前缀文本 | （空） |
| `-i` | `--output-index` | 输出选中项的序号而非内容 | 关闭 |
| `-n` | `--lines` | 可见行数（0 = 自适应，最大 20） | 0 |
| `-W` | `--width` | 窗口宽度，像素（0 = 铺满屏幕） | 0 |
| `-s` | `--font-size` | 字体大小 | 14.0 |
| `-f` | `--font` | 字体名称 | （系统默认） |
| `-m` | `--multi-select` | 多选模式（Tab 标记/取消，回车确认） | 关闭 |
| `-0` | `--null` | 多选结果用 NUL 分隔（配合 `xargs -0`） | 关闭 |
| `-P` | `--password` | 密码模式（隐藏输入，不显示列表） | 关闭 |
| | `--anchor` | 窗口位置：`top` `top-left` `top-center` `top-right` `bottom` `bottom-left` `bottom-center` `bottom-right` | top |
| `-b` | `--bg` | 背景颜色 | `#141522` |
| | `--fg` | 普通文字颜色 | `#D4DCF2` |
| | `--sbg` | 选中项背景颜色 | `#242838` |
| | `--sfg` | 选中项文字颜色 | `#D4DCF2` |
| | `--hfg` | 匹配字符高亮颜色 | `#C93B3B` |
| | `--prompt-bg` | 输入框背景颜色 | `#1B1D2B` |
| | `--prompt-fg` | 输入框文字颜色 | `#D4DCF2` |
| | `--prefix-fg` | 提示符前缀文字颜色 | `#D4DCF2` |
| | `--prefix-bg` | 提示符前缀背景颜色 | `#1B1D2B` |

## 快捷键

| 按键 | 功能 |
|------|------|
| 输入 | 过滤列表（模糊匹配 + 拼音） |
| 上 / Ctrl+P | 上移选中项 |
| 下 / Ctrl+N | 下移选中项 |
| 回车 | 确认选择 |
| Esc / Ctrl+C | 取消 |
| 退格 | 删除字符 |
| Ctrl+U | 全部清空 |
| Ctrl+W | 删除上一个单词 |
| Tab | 多选 |
| 鼠标左键 | 选中项目直接返回 |
| 鼠标右键 | 多选 |
| 滚轮 | 浏览列表 |

## 退出码

| 代码 | 含义 |
|------|------|
| 0 | 已选择 |
| 1 | 用户取消（Esc） |
| 3 | 错误（如无法连接 Wayland 显示服务器） |

## CJK 支持

- 原文模糊匹配
- 拼音全拼匹配（如输入 `beijing` 匹配 `北京`）
- 拼音首字母匹配（如输入 `bj` 匹配 `北京`）
- 多音字支持（如 `行` 同时匹配 `hang` 和 `xing`）
