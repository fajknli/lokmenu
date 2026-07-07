# 铺满（兼容原有行为）
echo -e "aaa\nbbb\nccc" | lokmenu -p "全宽: "
echo -e "aaa\nbbb\nccc" | lokmenu -p "全宽底: " --anchor bottom

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
