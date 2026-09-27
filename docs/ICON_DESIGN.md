# RC505 RS 图标

使用内置 imagegen 生成，未使用 CLI/API fallback。以 RC505 的五轨循环、合成器波形和 DAW 的简洁识别度为灵感，沿用深蓝、薄荷绿与蓝紫配色；没有直接使用参考品牌的标志。

- 原始透明 PNG：`assets/rc505-rs-icon-v1.png`（1254×1254，保留原始 alpha）。
- 程序内/窗口 PNG：`assets/rc505-rs-icon-v1-256.png`。
- Windows ICO：`assets/rc505-rs-icon-v1.ico`，包含16、24、32、48、64、128、256像素层。

ICO和256px版本只做缩放与格式转换；同一图标用于主程序、音频启动器、软件内品牌区和安装器。Windows可执行文件通过winres嵌入图标及版本信息，桌面快捷方式引用程序内图标。

## 最终生成提示词

```text
Use case: logo-brand.
Asset type: a finished Windows desktop software icon for an original music application called RC505 RS.
Create ONE exceptionally polished icon, not a contact sheet or mockup. Take category-level inspiration from the BOSS RC505 five-track live-looping instrument, Serum's precise waveform/synthesis visual language, and FL Studio's memorable, bold single-symbol identity, without copying any of their logos.
Composition: centered square icon, 1024x1024. A dark navy rounded-square tile with clean softly beveled edges, on a genuinely transparent background outside the rounded silhouette. The tile occupies about 88 percent of the canvas. Within it, a striking original emblem combines a continuous looping circular arc with exactly five rounded vertical fader/waveform bars of varying heights forming a musical pulse. Integrate the loop and the five bars into one coherent mark; avoid a generic equalizer logo. Clear negative space and strong balance. The emblem should remain legible at 32px and 16px.
Palette matches the app: deep navy #0F141C and #191F29, luminous mint #55DDBE as the primary accent, restrained periwinkle #8BA7FF as a secondary echo/return accent. Tasteful depth, precise crisp geometry, subtle highlights, minimal glow contained inside the tile. Premium modern synthesizer aesthetic, slightly futuristic and playful but professional.
No text, no letters or numbers, no fruit, no copied brand mark, no hardware product illustration, no tiny interface details, no gradients washing out the silhouette, no background scene, no external cast shadow, no watermark. Preserve true transparency in the outer corners.
```

