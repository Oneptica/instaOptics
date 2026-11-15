<p align="center">
  <img src="assets/hicolor/256x256/apps/com.oneptica.instaoptics.png" width="96" height="96" alt="instaOptics" />
</p>

<h1 align="center">instaOptics</h1>

<p align="center">
  <strong>免费开源的光学仿真软件，纯 Rust 编写。</strong><br />
  序列光学系统的设计与分析：改动镜头，所有分析随之更新。可以在桌面使用，可以用命令行写脚本，也可以通过内置的 MCP 服务让 AI agent 直接修改镜头。
</p>

<div align="center">

[![Windows](https://img.shields.io/badge/Windows-.msi-0078D4?style=for-the-badge&logo=windows&logoColor=white)][releases]
[![macOS](https://img.shields.io/badge/macOS-.dmg-000000?style=for-the-badge&logo=apple&logoColor=white)][releases]
[![Linux](https://img.shields.io/badge/Linux-AppImage-FCC624?style=for-the-badge&logo=linux&logoColor=black)][releases]
[![Web](https://img.shields.io/badge/网页版-instaoptics.oneptica.com-4F8FF7?style=for-the-badge)](https://instaoptics.oneptica.com)

</div>

<p align="center">
  <a href="https://github.com/Oneptica/instaOptics/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Oneptica/instaOptics/ci.yml?branch=main&label=CI" alt="CI" /></a>
  <a href="https://github.com/Oneptica/instaOptics/releases/latest"><img src="https://img.shields.io/github/v/release/Oneptica/instaOptics?label=release" alt="Release" /></a>
  <a href="https://github.com/Oneptica/instaOptics/releases"><img src="https://img.shields.io/github/downloads/Oneptica/instaOptics/total?label=downloads" alt="Downloads" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="MIT" /></a>
</p>

<p align="center"><a href="README.md">English</a></p>

![instaOptics：从 Zemax 文件导入 US 8,879,901 物镜，移动像面，优化后截距，再看 MTF 和三维布局](docs/screenshots/demo.gif)

*从 Zemax 文件导入 US 8,879,901 物镜（11 片，F/2）：移动像面，点列图随之变化；优化后截距；再看 MTF 和三维布局。由[演示脚本](examples/demo/record.py)录制，脚本调用的就是菜单使用的那套命令。*

## 下载

- **Windows：** `.msi` 安装包
- **macOS：** `.dmg` 磁盘映像（Apple 芯片和 Intel 通用）
- **Linux：** AppImage、`.deb` 或 `.tar.gz`

都在[发布页][releases]。安装后的程序启动时会检查新版本，Windows 安装版和 AppImage 会在下次启动时自动换成新版本。0.1.8 及以前是 Electron 版本，需要手动下载一次 0.2。

## 功能

- 序列光线追迹：球面、二次曲面、偶次非球面、反射镜、坐标断点，真实光线瞄准
- 孔径：入瞳直径、F 数、物方数值孔径、按光阑浮动；视场：角度、物高、像高
- 玻璃：肖特玻璃库、熔石英、氟化钙，以及 `nd/vd` 模型玻璃；可加载 OpticStudio 的 `.agf` 玻璃库（支持全部 13 种色散公式），带搜索窗口和阿贝图
- 点列图、像差曲线、FFT MTF、离焦 MTF、MTF 随视场、FFT PSF、波前图、场曲和畸变、色焦移、赛德尔像差、相对照度、足迹图
- 图像导出为 PNG，数据导出为 CSV
- 光束传播（物理光学）：高斯、超高斯、平顶光束穿过系统，可在轴上或沿某个视场的主光线传播，可看侧视图、截面、光束半径和单模光纤耦合效率
- 镀膜与偏振：每个面可设薄膜镀膜（MgF₂、四分之一波长层、高反膜、金属），可看反射率/透射率曲线，并做偏振光线追迹，给出光瞳上的透过率、二向衰减和相位延迟
- 成像仿真：可用测试图或任意图片
- 公差分析：灵敏度分析和蒙特卡洛
- 优化：阻尼最小二乘，评价函数可自定义操作数（焦距、总长、畸变、主光线角等），可按视场和结构分别设置
- 求解（边缘光线高度、关联）和多重结构
- 二维和三维布局图
- Zemax `.zmx` 导入导出

## 计算精度

同一个物镜在 `cargo test` 中和独立的开源光线追迹程序 [optiland](https://github.com/HarrisonKramer/optiland) 对比：EFL、BFL、入瞳直径和光瞳位置的相对差不超过 1.2e-5；54 根实际光线（3 个视场 × 3 个波长 × 6 个光瞳点）在像面上的交点与 optiland 相差不超过 0.02 µm。参考数据由 [examples/validation/us8879901.py](examples/validation/us8879901.py) 生成。

## 从源码构建

```sh
cargo run --release -p instaoptics                   # 桌面程序
cargo run --release -p instaoptics-cli -- commands   # 全部命令及参数
cargo test --workspace
```

各平台的打包脚本在 [packaging/](packaging/)。代码结构见 [docs/architecture.md](docs/architecture.md)。

## AI agent（MCP）

`instaoptics-cli mcp` 是一个 [Model Context Protocol](https://modelcontextprotocol.io) 服务。单独运行时它操作自己的镜头；加 `--connect` 时它驱动正在运行的程序，可以在界面里看着它改。Claude Code：`claude mcp add instaoptics -- instaoptics-cli mcp`。用 `instaoptics --control 7391` 启动程序，再用 `instaoptics-cli mcp --connect 127.0.0.1:7391` 操作当前打开的镜头。工具列表和示例见 [docs/mcp.md](docs/mcp.md)。

## 脚本

菜单里的每个操作都是一条命令，`instaoptics-cli` 运行的是同一套命令：

```sh
instaoptics-cli convert design.zmx design.iol
instaoptics-cli analyze lens.iol --request '{"kind":"mtf"}'
instaoptics-cli render lens.iol --view spot --out spot.png
instaoptics-cli run lens.iol --cmd 'variable {"surface":2,"parameter":"radius"}' --cmd 'optimize {"cycles":20}' --save out.iol
```

详见 [docs/cli.md](docs/cli.md) 和 [docs/control-protocol.md](docs/control-protocol.md)。文件格式为 `.iol`（JSON），长度单位 mm，波长 µm，角度为度。

[releases]: https://github.com/Oneptica/instaOptics/releases/latest
