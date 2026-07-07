# 左上角，固定宽度
echo -e "aaa\nbbb\nccc" | lokmenu -p "左上: " --anchor top-left -W 300

# 居中，固定宽度
echo -e "aaa\nbbb\nccc" | lokmenu -p "居中: " --anchor top-center -W 300

# 右上角，固定宽度
echo -e "aaa\nbbb\nccc" | lokmenu -p "右上: " --anchor top-right -W 300

# 左下角
echo -e "aaa\nbbb\nccc" | lokmenu -p "左下: " --anchor bottom-left -W 300

# 底部居中
echo -e "aaa\nbbb\nccc" | lokmenu -p "底中: " --anchor bottom-center -W 300

# 右下角
echo -e "aaa\nbbb\nccc" | lokmenu -p "右下: " --anchor bottom-right -W 300

--------

# -p 提示符
echo -e "alpha\nbeta\ngamma" | lokmenu -p "请输入: "

# -i 输出序号
echo -e "one\ntwo\nthree" | lokmenu -i -p "选择: "

# -n 显示行数（只显示 3 行）
echo -e "a\nb\nc\nd\ne\nf\ng" | lokmenu -n 3 -p "三行: "

# -W 固定宽度 400 像素
echo -e "hello\nworld" | lokmenu -W 400 -p "窄窗口: "

# -s 字体大小 20
echo -e "大字\n测试" | lokmenu -s 20 -p "大字: "

# -f 指定字体（用你系统上有的字体）
echo -e "hello\n你好" | lokmenu -f "monospace" -p "等宽: "

# -m 多选模式（按 Tab 标记，回车确认）
echo -e "aaa\nbbb\nccc\nddd" | lokmenu -m -p "多选: "

# -0 多选结果用 NUL 分隔（配合 xargs -0）
echo -e "file one\nfile two\nfile three" | lokmenu -m -0 -p "文件: " | xargs -0 echo "选中了:"

# -P 密码模式
lokmenu -P -p "密码: "

# --anchor bottom 窗口在底部
echo -e "shutdown\nreboot\nsuspend" | lokmenu -p "电源: " --anchor bottom

# -b 背景颜色（白色背景）
echo -e "hello\nworld" | lokmenu -b "#FFFFFF" --fg "#000000" --prompt-bg "#DDDDDD" --prompt-fg "#000000" -p "白底: "

# --fg 普通文字颜色（绿色文字）
echo -e "hello\nworld" | lokmenu --fg "#00FF00" -p "绿字: "

# --sbg 选中项背景（红色背景）
echo -e "aaa\nbbb\nccc" | lokmenu --sbg "#FF0000" --sfg "#FFFFFF" -p "红底选中: "

# --sfg 选中项文字颜色
echo -e "aaa\nbbb\nccc" | lokmenu --sfg "#FFFF00" -p "黄字选中: "

# --hfg 匹配高亮颜色（亮青色高亮）
echo -e "alpha\nbeta\ngamma" | lokmenu --hfg "#00FFFF" -p "高亮: "
# 输入 a 后，匹配到的 a 会以亮青色显示

# --prompt-bg 输入框背景（深蓝色输入框）
echo -e "hello\nworld" | lokmenu --prompt-bg "#000088" --prompt-fg "#FFFFFF" -p "蓝底输入: "

# --prompt-fg 输入框文字颜色
echo -e "hello\nworld" | lokmenu --prompt-fg "#FF8800" -p "橙色输入: "

-------------

# 全参数组合：底部锚定、窄窗口、自定义配色、多选、NUL 分隔
echo -e "文档/报告.txt\n文档/笔记.txt\n图片/照片.png\n音乐/歌曲.mp3" \
  | lokmenu -m -0 -n 5 -W 600 -s 16 \
    -p "选择文件: " --anchor bottom \
    -b "#1a1b26" --fg "#a9b1d6" \
    --sbg "#3b4261" --sfg "#c0caf5" \
    --hfg "#ff9e64" \
    --prompt-bg "#24283b" --prompt-fg "#c0caf5" \
  | xargs -0 echo "选中了:"

-----------------

# 1. 路径边界匹配测试（你的 niri 问题）
echo -e "/home/user/.config/niri/config\n/home/fajknli/.config/niri\n/home/fajknli/.local/nirixxx\n/home/user/niri-theme\n/home/user/.config/waybar/niri.conf" | lokmenu -p "路径: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#d70000" --prefix-fg "#11121c"

# 2. 中文 + 拼音测试
echo -e "浏览器\n播放器\n音乐播放器\n图书管理器\n文件管理器\n蓝牙设置\n网络设置\n系统监控\n终端模拟器\n屏幕截图" | lokmenu -p "应用: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#d70000" --prefix-fg "#11121c"

# 3. 中英混合 + 路径测试
echo -e "/home/音乐/播放器/周杰伦.mp3\n/home/下载/VLC播放器\n~/Documents/项目报告.pdf\n~/.config/音乐/settings.json\n/home/user/.local/share/播放器\n/opt/VLC-播放器/lib/vlc\n/home/music/摇滚/播放列表.m3u" | lokmenu -p "文件: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#d70000" --prefix-fg "#11121c"

# 4. 短文本密集测试（纯中文）
echo -e "你好世界\n你好\n你好的\n世界你好\n你\n好\n你好吗\n世界" | lokmenu -p "搜索: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#005f5f" --prefix-fg "#ffffff"

# 5. 纯英文路径测试（边界 vs 非边界）
echo -e "src/main.rs\nsrc/matcher.rs\nsrc/render.rs\nsrc/state.rs\nCargo.lock\nCargo.toml\nREADME.md\n.github/workflows/ci.yml\n.gitignore" | lokmenu -p "open file: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#3a3a5c" --prefix-fg "#e0e0ff"

# 6. 长路径截断测试
echo -e "/home/user/.local/share/flatpak/app/com.github.nickvision.tubeconverter/current/active/files/share/icons/hicolor/256x256/apps/com.github.nickvision.tubeconverter.png\n/home/user/.config/niri/config\n/usr/share/icons/Papirus/48x48/apps/firefox.svg" | lokmenu -p "图标: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#8b0000" --prefix-fg "#ffd700"

# 7. 底部锚点 + 中文
echo -e "蓝牙\n蓝牙耳机\n蓝牙音箱\n蓝牙设置\n蓝牙管理\n蓝牙共享\n蓝牙设备列表\n蓝牙连接历史" | lokmenu -p "蓝牙: " --anchor bottom -f "0xProto Nerd Font" -s 12 --prefix-bg "#004488" --prefix-fg "#ffffff"

# 8. 连续性打分测试（验证 DP 是否正确选路径）
echo -e "configure\nniri\nconfig/niri\nniri-config\n.cofnig/niri\n.niri/config\nwayland-niri\nx11-niri\nconfigure-niri" | lokmenu -p "test: " -f "0xProto Nerd Font" -s 12 --prefix-bg "#2d5a27" --prefix-fg "#ffffff"
