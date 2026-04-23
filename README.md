# Vanity Wallet Generator 使用说明

这是一个在 Windows PowerShell 里运行的 **EVM 靓号钱包地址生成器**。

当前项目已经精简为 **Rust 原生 `.exe` 版**，不再包含 Node.js 版本。平时只需要记住一个启动脚本：

```powershell
.\start-native.ps1
```

它会不断随机生成私钥，推导 `0x...` 地址，然后检查地址是否符合你设置的前缀或后缀。命中后，会把地址和私钥保存到 `results` 文件夹。

## 支持范围

适用：

- Ethereum
- BSC / BNB Chain
- Polygon
- Arbitrum
- Optimism
- Base
- 其他使用 EVM `0x...` 地址格式的链

不适用：

- Bitcoin
- Solana
- Tron 原生 `T...` 地址
- 非 `0x...` 格式的钱包地址

## 重要安全提醒

请先看这一段。

- 私钥就是资产控制权，任何人看到私钥都可以转走这个地址里的资产。
- 不要把私钥发给别人，不要贴到网页、聊天软件、截图、云笔记里。
- 不要使用网上的靓号地址生成器，私钥可能会泄露。
- 命中后请立刻备份 `PrivateKey`。
- 正式转入大额资产前，建议先小额测试。
- 如果运行测试时加了 `-RedactPrivateKey`，结果里不会保存可用私钥。

## 工作原理

这个工具不是“指定生成某个地址”，而是本地暴力随机搜索：

1. 随机生成 32 字节私钥。
2. 用 secp256k1 推导公钥。
3. 用 Keccak-256 计算 EVM 地址。
4. 判断地址是否满足前缀或后缀规则。
5. 不满足就继续生成。
6. 命中后保存地址和私钥，然后停止。

每固定 1 个十六进制字符，难度乘以 16。

```text
后缀 0000       平均约 65,536 次尝试
后缀 000000     平均约 16,777,216 次尝试
后缀 00000000   平均约 4,294,967,296 次尝试
后缀 000000000  平均约 68,719,476,736 次尝试
```

这些是平均值，不是保证值。运气好可能很快，运气差可能跑几倍时间。

## 文件结构

项目位置：

```text
C:\Users\Administrator\Desktop\vanity-wallet-generator
```

主要文件：

```text
start-native.ps1         启动原生版，平时主要运行它
Build-Native.ps1         编译 Rust 原生 exe
Measure-NativeSpeed.ps1  测速脚本
Get-Status.ps1           查看当前或最近一次状态
bin\vanity-native.exe    编译后的 Windows 可执行文件
native\vanity-native\    Rust 源码
results\                 命中结果保存位置
state\                   状态文件保存位置
logs\                    日志保存位置
```

## 第一次运行

打开 Windows PowerShell，进入项目目录：

```powershell
cd "$env:USERPROFILE\Desktop\vanity-wallet-generator"
```

如果提示脚本不能运行，先在当前窗口临时放行：

```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
```

然后运行短测试：

```powershell
.\start-native.ps1 -Suffix "0000" -Workers 4 -PreventSleep
```

如果 `bin\vanity-native.exe` 不存在，脚本会自动调用：

```powershell
.\Build-Native.ps1
```

编译成功后会生成：

```text
bin\vanity-native.exe
```

你也可以手动编译：

```powershell
.\Build-Native.ps1
```

## 正式运行

搜索 8 个 `0` 后缀：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 8 -PreventSleep
```

搜索 9 个 `8` 后缀：

```powershell
.\start-native.ps1 -Suffix "888888888" -Workers 8 -PreventSleep
```

只要求前缀：

```powershell
.\start-native.ps1 -Prefix "000000" -Workers 8 -PreventSleep
```

同时要求前缀和后缀：

```powershell
.\start-native.ps1 -Prefix "0000" -Suffix "000000" -Workers 8 -PreventSleep
```

如果电脑变卡，可以减少 worker：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 4 -PreventSleep
```

如果 CPU 核心很多，可以尝试增加 worker：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 12 -PreventSleep
```

一般建议给系统留 1 到 2 个核心，不要把电脑压到完全无法操作。

## 参数说明

### `-Prefix`

地址前缀，不包含 `0x`。

```powershell
.\start-native.ps1 -Prefix "0000" -Workers 8
```

也可以写 `0x`，脚本会自动处理：

```powershell
.\start-native.ps1 -Prefix "0x0000" -Workers 8
```

### `-Suffix`

地址后缀，不包含 `0x`。

默认值是：

```text
00000000
```

也就是说，直接运行：

```powershell
.\start-native.ps1
```

默认就是搜索后缀 `00000000`。

### `-Workers`

并行线程数量。

常见选择：

```text
4    占用较低
8    推荐起点
12   CPU 核心较多时可以尝试
```

### `-PreventSleep`

运行期间防止 Windows 睡眠。长期挂着跑时建议加上：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 8 -PreventSleep
```

它不会阻止手动关机，也不一定覆盖所有系统电源策略。长期运行时，最好同时检查 Windows 电源设置。

### `-RedactPrivateKey`

隐藏结果文件里的私钥，只适合测试：

```powershell
.\start-native.ps1 -Suffix "0000" -Workers 4 -RedactPrivateKey
```

正式搜索不要加。否则命中后结果文件里会显示：

```text
PrivateKey: [redacted by --redact-private-key]
```

这样这个钱包就无法使用。

### `-PlainOutput`

恢复逐行输出模式。

默认情况下，状态会在同一行里实时刷新，不会一直刷屏。只有你想保留终端日志时才建议加：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 8 -PreventSleep -PlainOutput
```

### `-NoBuild`

跳过自动编译。

如果已经存在 `bin\vanity-native.exe`，可以使用：

```powershell
.\start-native.ps1 -Suffix "0000" -Workers 4 -NoBuild
```

如果 exe 不存在，不要加这个参数。

### 高级参数

通常不需要修改：

```text
-StatusIntervalSeconds   状态刷新间隔，默认 5 秒
-BatchSize               每批生成数量，默认 1024
-MaxSeconds              最多运行多少秒，默认 0 表示不限制
-CaseSensitive           按 EIP-55 checksum 大小写精确匹配，一般不要加
```

## 运行时怎么看

启动后会先显示任务信息：

```text
Native EVM vanity search
Run ID: 20260423-120000000
Target: prefix '-' suffix '00000000'
Workers: 8
Average attempts estimate: 4,294,967,296
Status updates will refresh on one line. Use -PlainOutput for scrolling output.
```

然后状态会在同一行里刷新：

```text
[12:00:05] attempts=1,314,816 rate=228,138/s runtime=00:00:06 workers=8/8
```

字段含义：

```text
attempts   已尝试次数
rate       当前每秒生成/检查地址数量
runtime    本次运行时长
workers    当前 worker 数量
```

默认 5 秒刷新一次。想更实时可以改成 1 秒：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 8 -PreventSleep -StatusIntervalSeconds 1
```

## 测速

只测速，不等待命中：

```powershell
.\Measure-NativeSpeed.ps1 -Workers 8 -Seconds 20
```

它会使用一个几乎不可能在短时间内命中的目标，只跑固定秒数，用来观察 `rate`。

这台机器实测 8 个 worker 大约：

```text
190,000 到 230,000 地址/秒
```

不同电脑、后台负载、Windows 电源模式都会影响速度。

## 时间预估

按 `200,000 地址/秒` 粗略估算：

```text
后缀 0000       约 0.3 秒
后缀 000000     约 1.4 分钟
后缀 00000000   约 6 小时
后缀 000000000  约 4 天
```

这仍然是平均值，不是保证值。

## 另开窗口查看状态

可以另开一个 PowerShell：

```powershell
cd "$env:USERPROFILE\Desktop\vanity-wallet-generator"
.\Get-Status.ps1
```

示例：

```text
Run ID:        20260423-120000000
Engine:        native-rust
Target:        prefix '' suffix '00000000'
Attempts:      123456789
Rate:          220000 / sec
Runtime:       00:09:21
Workers:       8 / 8
Restarts:      0
Matched:       False
Last updated:  04/23/2026 12:30:00
```

## 如何停止

在运行窗口按：

```text
Ctrl+C
```

如果之后想继续搜，重新运行同一条命令即可：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 8 -PreventSleep
```

这个搜索是随机抽样，不需要恢复进度。停止后再启动，就是继续随机搜索。

## 命中后看结果

结果保存在：

```text
C:\Users\Administrator\Desktop\vanity-wallet-generator\results
```

每次命中会生成：

```text
matched-wallet-native-<run-id>.txt
```

同时会更新：

```text
matched-wallet-latest.txt
```

结果文件大概长这样：

```text
EVM Vanity Wallet Match

Engine: native-rust
RunId: 20260423-120000000
FoundAt: 2026-04-23T12:00:00+08:00
Address: 0x...
PrivateKey: 0x...
Prefix: -
Suffix: 00000000
CaseSensitive: false
EstimatedAverageAttempts: 4294967296
TotalAttemptsObserved: ...
WorkerId: ...
WorkerAttemptsThisRun: ...
```

最重要的是：

```text
Address     钱包地址，可以收款
PrivateKey  私钥，可以导入钱包，也可以控制资产
```

请务必备份 `PrivateKey`。

## 常见问题

### 怎么上传到 GitHub

推荐用 Git 命令上传，不建议直接在 GitHub 网页里拖整个文件夹。因为网页拖拽不会读取 `.gitignore`，容易把 `results`、`logs`、`state` 或 `bin` 一起传上去。

先在 GitHub 网页新建一个空仓库：

```text
Repository name: vanity-wallet-generator
Visibility: Public 或 Private 都可以
不要勾选 Add a README file
不要勾选 Add .gitignore
不要勾选 Choose a license
```

创建后，GitHub 会给你一个仓库地址，类似：

```text
https://github.com/你的用户名/vanity-wallet-generator.git
```

然后在 PowerShell 里运行：

```powershell
cd "$env:USERPROFILE\Desktop\vanity-wallet-generator"
git init
git add .
git commit -m "Initial native vanity wallet generator"
git branch -M main
git remote add origin https://github.com/你的用户名/vanity-wallet-generator.git
git push -u origin main
```

注意把上面的仓库地址换成你自己的。

如果 `git commit` 提示没有配置用户名和邮箱，先运行：

```powershell
git config --global user.name "你的名字"
git config --global user.email "你的邮箱"
```

然后重新运行：

```powershell
git commit -m "Initial native vanity wallet generator"
git push -u origin main
```

上传前可以检查哪些文件会被提交：

```powershell
git status --short
```

正常情况下，不应该看到这些目录：

```text
results
logs
state
bin
native\vanity-native\target
```

这些目录已经写进 `.gitignore`，不会被 Git 默认提交。

### 提示脚本无法运行

运行：

```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
```

这个设置只影响当前 PowerShell 窗口，关掉窗口后会恢复。

### 提示找不到 Cargo 或 Rust

如果第一次编译时提示找不到 `cargo`，说明 Rust 编译环境不在 PATH 里。

这台机器已经编译好了：

```text
bin\vanity-native.exe
```

如果以后换机器，需要安装 Rust，或者把已经编译好的 `bin\vanity-native.exe` 一起带过去。

### 命中了，但结果里没有私钥

检查是不是加了：

```powershell
-RedactPrivateKey
```

这个参数只适合测试。正式运行不要加。

### 电脑变卡

减少 worker 数量：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 4 -PreventSleep
```

### 跑很久没出

这是正常的。`00000000` 后缀平均需要约 42.9 亿次尝试，`888888888` 这种 9 位后缀平均需要约 687 亿次尝试。

可以先测速：

```powershell
.\Measure-NativeSpeed.ps1 -Workers 8 -Seconds 20
```

再根据实际 `rate` 估算时间。

## 推荐流程

1. 进入目录：

```powershell
cd "$env:USERPROFILE\Desktop\vanity-wallet-generator"
```

2. 短测试：

```powershell
.\start-native.ps1 -Suffix "0000" -Workers 4 -PreventSleep
```

3. 测速度：

```powershell
.\Measure-NativeSpeed.ps1 -Workers 8 -Seconds 20
```

4. 正式跑：

```powershell
.\start-native.ps1 -Suffix "00000000" -Workers 8 -PreventSleep
```

5. 命中后打开：

```text
results\matched-wallet-latest.txt
```

6. 备份 `PrivateKey`。

7. 小额测试收款和导入钱包。

8. 确认无误后再考虑正式使用。
