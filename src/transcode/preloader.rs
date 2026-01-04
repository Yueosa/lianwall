use super::config::TranscodeConfig;
use super::encoder::transcode_async;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::thread::JoinHandle;

/// 预加载转码队列管理器
pub struct PreloadQueue {
    /// 待转码队列
    pending: VecDeque<PathBuf>,
    /// 进行中的任务 (原始路径 -> 任务句柄)
    in_progress: HashMap<PathBuf, JoinHandle<Result<(), String>>>,
    /// 已完成的缓存
    completed: HashSet<PathBuf>,
    /// 最大并发转码数
    #[allow(dead_code)]
    max_concurrent: usize,
}

impl PreloadQueue {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            in_progress: HashMap::new(),
            completed: HashSet::new(),
            max_concurrent: max_concurrent.max(1),
        }
    }

    /// 添加预加载任务
    pub fn add(&mut self, videos: Vec<PathBuf>) {
        for video in videos {
            // 跳过已经在队列中或已完成的
            if !self.pending.contains(&video)
                && !self.in_progress.contains_key(&video)
                && !self.completed.contains(&video)
            {
                self.pending.push_back(video);
            }
        }
    }

    /// 批量添加预加载任务（使用 TranscodeConfig）
    pub fn enqueue_batch(&mut self, videos: Vec<PathBuf>, _config: &TranscodeConfig) {
        // 当前实现只需要路径，config 在实际转码时使用
        self.add(videos);
    }

    /// 检查任务状态，清理完成的任务
    #[allow(dead_code)]
    pub fn poll(&mut self) {
        let mut finished = Vec::new();

        for (path, handle) in &self.in_progress {
            if handle.is_finished() {
                finished.push(path.clone());
            }
        }

        for path in finished {
            if let Some(handle) = self.in_progress.remove(&path) {
                match handle.join() {
                    Ok(Ok(_)) => {
                        self.completed.insert(path.clone());
                    }
                    Ok(Err(e)) => {
                        eprintln!("转码失败 {}: {}", path.display(), e);
                    }
                    Err(_) => {
                        eprintln!("转码线程崩溃: {}", path.display());
                    }
                }
            }
        }
    }

    /// 启动下一个待转码任务（如果有空闲）
    #[allow(dead_code)]
    pub fn start_next(
        &mut self,
        get_cache_path_fn: impl Fn(&PathBuf) -> (PathBuf, TranscodeConfig),
    ) {
        while self.in_progress.len() < self.max_concurrent {
            if let Some(input) = self.pending.pop_front() {
                let (output, config) = get_cache_path_fn(&input);

                // 检查缓存是否已存在
                if output.exists() {
                    self.completed.insert(input);
                    continue;
                }

                println!(
                    "后台预转码: {}",
                    input.file_name().unwrap().to_string_lossy()
                );

                let handle = transcode_async(input.clone(), output, config);
                self.in_progress.insert(input, handle);
            } else {
                break;
            }
        }
    }

    /// 获取队列状态
    #[allow(dead_code)]
    pub fn status(&self) -> (usize, usize, usize) {
        (
            self.pending.len(),
            self.in_progress.len(),
            self.completed.len(),
        )
    }

    /// 清空队列
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.pending.clear();
        // 注意：不清理 in_progress，让正在进行的任务完成
    }
}
