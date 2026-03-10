# RC505 for Free!

## 1. Brief Intro

This is an open source local app that simulates the BOSS RC505 MK2, or RC505. I can not afford an RC505, which basically costs around 5000 RMB in China, and all free apps I have used are just looper with very simple function and FX. So I decided to develop a free software RC505 with as many functions as I can handle. 

---

## 2. What is RC505

简单来说，它是目前市面上**循环创作（Looping）的行业标杆**。

* **五轨道系统**：拥有五个独立的立体声循环轨道，每个轨道都有独立的录制、播放、停止按键和音量推子。
* **双特效引擎**：支持 Input FX（录制时生效）和 Track FX（播放时生效），每个部分有 4 个可快速切换的效果。
* **同步与节奏**：强大的内置鼓机和量化功能，确保不同长度的 Loop 能够完美对齐。
* **操控中心**：虽然是桌面设备，但它本质上是一个高度可定制的音频工作站，支持复杂的 MIDI 映射。

---

## 3. About the Project

It is implemented with Rust. It only supports Windows OS. I have literally no experience in developing an mobile app, and I wish it will be extended and supported across the platforms. It would be perfect if it were available on IPad. 

### Mannual (ver 0.1.0) 

It can only be operated by keyboard. The mannual is listed below.

1. When you lauch the app, it shows the initial interface for you to create or delete projects. You can press `UpArrow` or `DownArrow` to select projects. If you select the bottom item, you can press `Enter` and input the name to create a new project. Press `Delete` to delete the selected project. Press `Enter` to enter in the selected project.

2. When you enter in the app, you can see a simple loop panel. Mention that the UI are just for a similar appearance, and not clickable lol. There are 2 status for operation, which are **Loop** and **Screen**. Press `s` to switch the status. If the screen above has a red strove, you are in the **Screen** status. 

3. When you are in `Loop` status, you basically control the record of tracks. Press `1-5` to record/dub/play the corresponding track (i.e. the track with the same number). If the track is empty, it will record the input audio. If the track is playing, it will dub the input audio. If the track is pause, it will be played immediately. If the track is recording or playing, it will be played at the next beat time. Press `F1-F5` to pause the corresponding track. Press `RightArrow` and `LeftArrow` to select the track. Press `Delete` to delete the audio in the selected track. 

    In `Loop` status you can also controls the fx. Press `T` to toggle between selecting banks or togglingg fx. So far you can only controls Input FX. When you are selecting banks (buttons are blue), you can press `QWER` to switch among 4 banks. When you are toggling input fx (buttons are red), you can press `QWER` to controls the on/off of the 4 input fx respectively. 

4. When you are in `Screen` status, you can controls the settings/configs of the project. 

- If you press `B`, you get into the beat configs. 