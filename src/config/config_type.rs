// src/config/config_types.rs

use eframe::egui;

// trait for a set of configs, like beat configs, system configs, etc.
pub trait ConfigSet {
    fn next(&mut self);
    fn prev(&mut self);
    fn confirm(&mut self);
}

// Struct for a single config, define different types, like enum, numeric...
/// Configs with Numeric Input 
pub struct NumericConfig {
    /// 设置的显示标签
    pub label: String,
    /// 当前编辑的值
    pub value: usize,
    /// 数值输入时的缓冲（用户正在输入的数字）
    pub buffer: String,
}

impl NumericConfig {
    pub fn new(label: &str, initial_value: usize) -> Self {
        Self {
            label: label.to_string(),
            value: initial_value,
            buffer: String::new(),
        }
    }

    /// 确认设置，返回最终的值
    pub fn confirm(&mut self) -> usize {
        // self.buffer.clear();
        self.value
    }

    /// Input numeric value
    pub fn input(&mut self, i: &egui::InputState, max_num: usize) {
        let num_keys = [
            (egui::Key::Num0, '0'),
            (egui::Key::Num1, '1'),
            (egui::Key::Num2, '2'),
            (egui::Key::Num3, '3'),
            (egui::Key::Num4, '4'),
            (egui::Key::Num5, '5'),
            (egui::Key::Num6, '6'),
            (egui::Key::Num7, '7'),
            (egui::Key::Num8, '8'),
            (egui::Key::Num9, '9'),
        ];
        let mut buffer_changed = false;
        for (key, digit) in num_keys.iter() {
            if i.key_pressed(*key) {
                self.buffer.push(*digit);
                buffer_changed = true;
            }
        }
        
        // Backspace 删除最后一个字符
        if i.key_pressed(egui::Key::Backspace) {
            self.buffer.pop();
            buffer_changed = true;
        }

        if buffer_changed {
            if let Ok(value) = self.buffer.parse::<usize>() {
                if value <= max_num {
                    self.value = value;
                } else {
                    self.buffer.pop();
                }
            }
        }
    }
}

/// Config with Enumerate Input, adjusted by up-down
pub struct EnumConfig<T> {
    pub label: String,
    pub value: T,
    pub options: Vec<T>, // Directly Cache Options
}

impl<T: PartialEq + Clone> EnumConfig<T> {
    pub fn new(label: &str, value: T, options: Vec<T>) -> Self {
        Self {
            label: label.to_string(),
            value,
            options,
        }
    }

    pub fn confirm(& self) -> T {
        self.value.clone()
    }

    pub fn next(&mut self) {
        if let Some(pos) = self.options.iter().position(|x| x == &self.value) {
            self.value = self.options[(pos + 1) % self.options.len()].clone();
        }
    }

    pub fn prev(&mut self) {
        if let Some(pos) = self.options.iter().position(|x| x == &self.value) {
            self.value = self.options[(pos + self.options.len() - 1) % self.options.len()].clone();
        }
    }
}

