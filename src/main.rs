use serde::{Deserialize, Serialize};
use serde_json;
use std::path::PathBuf;
use std::fs::{File, OpenOptions};
use std::io::{Write, Seek, SeekFrom, ErrorKind, Read};

fn main() {
    println!("Type 'help' to view existing commands");
    let mut app = App::new();
    app.run();
}

struct App {
    current_file: Option<File>,
    tasks: Vec<Task>
}
impl App {
    fn new() -> Self {
        Self {current_file: None, tasks: Vec::new()}
    }
    fn run(&mut self) {
        'inner: loop {
            let act = match stdin_to_action() {
                Ok(act) => act,
                Err(e) => {
                    println!("If you get stuck, please type help\n Error: {e}");
                    continue;
                }
            };
            match self.start(&act) {
                Ok(AppOk::CalledExit) => break,
                Ok(AppOk::CalledHelp) => continue,
                Err(e) => {
                    println!("Please open or create a file before doing anything\n Error:{e:?}");
                    continue;
                }
                _ => {}
            };
            loop {
                let last = match self.handle_file() {
                    Ok(l) => l,
                    Err(e) => {
                        println!("An error occurred: {e}");
                        break;
                    }
                };
                match self.current_file.as_ref().unwrap().sync_data() {
                    Ok(_) => {}
                    Err(e) => {
                        println!("An error occurred: {e}");
                        break;
                    }
                };
                match self.handle_last(&last) {
                    Ok(AppOk::CalledClose) => break,
                    Ok(AppOk::None) => continue,
                    Ok(AppOk::CalledExit) => break 'inner,
                    Ok(AppOk::CalledDelete) => break,
                    Err(AppError::OpenFailed(e)) => {
                        println!("Failed to open list: {e}");
                        break;
                    }
                    Err(AppError::DeleteFailed(e)) => {
                        println!("Failed to delete list: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    fn start(&mut self, act: &Action) -> Result<AppOk, AppError> {
        match act {
            Action::CreateList(p) => {
                self.current_file = match OpenOptions::new().create_new(true).read(true).write(true).open(p) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        return Err(AppError::OpenFailed(e))
                    }
                };
                Ok(AppOk::None)
            }
            Action::Open(p) => {
                self.current_file = match OpenOptions::new().read(true).write(true).open(p) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        return Err(AppError::OpenFailed(e))
                    }
                };
                Ok(AppOk::None)
            }
            Action::Help => {
                help();
                Ok(AppOk::CalledHelp)
            }
            Action::Exit => Ok(AppOk::CalledExit),
            _ => Err(AppError::FileNotOpened)
        }
    }
    fn handle_file(&mut self) -> Result<Action, Box<dyn std::error::Error>> {
        let mut last_act = Action::None;
        self.tasks = {
            let mut res = String::new();
            self.current_file.as_mut().unwrap().read_to_string(&mut res)?;
            if res.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str::<Vec<Task>>(&res)?
            }
        };
        loop {
            println!("====== Your goals ======");
            for (idx, t) in self.tasks.iter().enumerate() {
                println!("{}) {}", idx + 1, t);
            }
            let act = match stdin_to_action() {
                Ok(act) => act,
                Err(e) => {
                    println!("If you get stuck, please type help.\n{e}");
                    continue;
                }
            };
            match act {
                Action::Open(p) => {
                    last_act = Action::Open(p);
                    break;
                }
                Action::CreateList(p) => {
                    last_act = Action::CreateList(p);
                    break;
                }
                Action::CreateTask(s, p) => {
                    let new_task = Task::new(s, p);
                    self.tasks.push(new_task);
                }
                Action::DeleteList(p) => {
                    last_act = Action::DeleteList(p);
                    break;
                }
                Action::DeleteTask(i) => {
                    if i != 0 && i - 1 < self.tasks.len() {
                        self.tasks.remove(i - 1);
                    } else {
                        println!("Can't find a task with index {}", i);
                    }
                }
                Action::Complete(i) => {
                    let t = match self.tasks.get_mut(i - 1) {
                        Some(t) => t,
                        None => {
                            println!("Can't find a task with index {}", i);
                            continue
                        }
                    };
                    t.is_completed = true;
                }
                Action::Help => help(),
                Action::Close => {
                    break;
                }
                Action::Save => {
                    self.save_to_file()?;
                }
                Action::Exit => {
                    last_act = Action::Exit;
                    break;
                }
                Action::None => {
                    continue
                }
            }
        }
        self.save_to_file()?;

        Ok(last_act)
    }
    fn handle_last(&mut self, act: &Action) -> Result<AppOk, AppError> {
        match act {
            Action::None => Ok(AppOk::CalledClose),
            Action::Open(p) => {
                self.current_file = match OpenOptions::new().read(true).write(true).open(p) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        return Err(AppError::OpenFailed(e))
                    }
                };
                Ok(AppOk::None)
            }
            Action::CreateList(p) => {
                self.current_file = match OpenOptions::new().create_new(true).read(true).write(true).open(p) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        return Err(AppError::OpenFailed(e))
                    }
                };
                Ok(AppOk::None)
            }
            Action::DeleteList(p) => {
                match std::fs::remove_file(p) {
                    Ok(_) => Ok(AppOk::CalledDelete),
                    Err(e) => {
                        Err(AppError::DeleteFailed(e))
                    }
                }
            }
            Action::Exit => Ok(AppOk::CalledExit),
            _ => unreachable!()
        }
    }
    fn save_to_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(f) = self.current_file.as_mut() {
            f.set_len(0)?;
            f.seek(SeekFrom::Start(0))?;
            let json = serde_json::to_string_pretty(&self.tasks)?;
            f.write_all(json.as_bytes())?;
            f.flush()?;
            return Ok(());
        }
        Err(std::io::Error::new(ErrorKind::Other, "List isn't opened").into())
    }
}
#[derive(Debug)]
enum AppError {
    FileNotOpened,
    OpenFailed(std::io::Error),
    DeleteFailed(std::io::Error),
}
#[derive(Debug)]
enum AppOk {
    CalledHelp,
    CalledExit,
    CalledClose,
    CalledDelete,
    None,
}
fn help() {
    let help_info = String::from("Commands:\n\
    * create list \"path\\to\\new\\list.json\" - will create and open a new to do list\n\
    * create task \"Task description\" priority - will create a new task in opened list\n\
    * open \"path\\to\\existing\\list.json\" - will open an existing to do list\n\
    * close - will close and save opened to do list\n\
    * delete list \"path\\to\\existing\\list.json\" - will delete an existing to do list\n\
    * delete task task_index - will delete a task in opened list\n\
    * complete task_index - will mark a task in opened list as complete\n\
    * help - will display this menu\n\
    * save - will save list info to\n\
    * exit - will exit the program(you should use this ff you don't want to lose all new tasks)");
    println!("{help_info}");
}
#[derive(Serialize, Deserialize, Debug)]
struct Task {
    pub priority: u8,
    pub title: String,
    pub is_completed: bool,
}
impl Task {
    fn new(title: String, priority: u8) -> Self {
        Self { priority, title, is_completed: false }
    }
}
impl std::fmt::Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "* {}({}) - {}", self.title, self.priority, if self.is_completed { "completed" } else { "not completed" })
    }
}

#[derive(Debug)]
enum Token {
    Open, // Open a new json file with tasks and close previous
    Close, // Close json file
    Create, // Create a new task or list
    Delete,
    Complete, // Mark task as completed
    Identifier(String), // Path to the file or name of the task
    Number(isize), // id or priority of the task,
    Help, // output help menu,
    Exit, // Exit from program
    Save, // Saves data
    Task,
    List,
}

fn lex(s: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' => continue,
            _ if c.is_alphabetic() => {
                let mut buffer = String::from(c);
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        buffer.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                match buffer.to_lowercase().as_str() {
                    "create" => tokens.push(Token::Create),
                    "open" => tokens.push(Token::Open),
                    "close" => tokens.push(Token::Close),
                    "complete" => tokens.push(Token::Complete),
                    "help" => tokens.push(Token::Help),
                    "exit" => tokens.push(Token::Exit),
                    "task" => tokens.push(Token::Task),
                    "list" => tokens.push(Token::List),
                    "save" => tokens.push(Token::Save),
                    "delete" => tokens.push(Token::Delete),
                    _ => return Err(format!("Invalid token: {}", buffer)),
                }
            },
            _ if c.is_numeric() => {
                let mut buffer = String::from(c);
                while let Some(&c) = chars.peek() {
                    if c.is_numeric() {
                        buffer.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Number(match buffer.parse::<isize>() {
                    Ok(i) => i,
                    Err(_) => return Err(format!("Invalid number: {}", buffer)),
                }));
            }
            '\"' | '\'' => {
                let mut buffer = String::from("");
                while let Some(&c) = chars.peek() {
                    if c != '\'' && c != '\"' {
                        buffer.push(c);
                        chars.next();
                    } else {
                        chars.next();
                        break;
                    }
                }
                tokens.push(Token::Identifier(buffer));
            }
            _ => return Err(format!("Invalid token: {}", c)),
        }
    }
    
    Ok(tokens)
}
enum Action {
    Open(PathBuf),
    CreateList(PathBuf),
    CreateTask(String, u8),
    DeleteList(PathBuf),
    DeleteTask(usize),
    Complete(usize),
    Help,
    Close,
    Exit,
    Save,
    None,
}
fn parse_target(tokens: &Vec<Token>) -> Result<Action, String> {
    match tokens.get(0) {
        Some(&Token::Open) => {
            Ok(Action::Open(match tokens.get(1) {
                Some(Token::Identifier(s)) => s.into(),
                Some(t) => return Err(format!("Unexpected token {t:?}. Expected Identifier")),
                None => return Err("Unexpected end of statement".to_string()),
            }))
        }
        Some(&Token::Create) => {
            match tokens.get(1) {
                Some(Token::List) => {
                    let ident = match tokens.get(2).ok_or("Unexpected end of statement".to_string())? {
                        Token::Identifier(s) => s.into(),
                        t => return Err(format!("Unexpected token {t:?}. Expected Identifier")),
                    };
                    Ok(Action::CreateList(ident))
                }
                Some(Token::Task) => {
                    let name = match tokens.get(2).ok_or("Unexpected end of statement".to_string())? {
                        Token::Identifier(s) => s.into(),
                        t => return Err(format!("Unexpected token {t:?}. Expected Identifier")),
                    };
                    let priority = match tokens.get(3).ok_or("Unexpected end of statement".to_string())? {
                        Token::Number(i) => *i as u8,
                        t => return Err(format!("Unexpected token {t:?}. Expected Number")),
                    };
                    Ok(Action::CreateTask(name, priority))
                }
                Some(t) => Err(format!("Unexpected token {t:?}. Expected Task/List")),
                None => Err("Unexpected end of statement".to_string()),
            }
        }
        Some(&Token::Delete) => {
            match tokens.get(1) {
                Some(Token::List) => {
                    let ident = match tokens.get(2).ok_or("Unexpected end of statement".to_string())? {
                        Token::Identifier(s) => s.clone(),
                        t => return Err(format!("Unexpected token {t:?}. Expected Identifier")),
                    };
                    Ok(Action::DeleteList(ident.into()))
                }
                Some(Token::Task) => {
                    let idx = match tokens.get(2).ok_or("Unexpected end of statement".to_string())? {
                        Token::Number(i) => *i as usize,
                        t => return Err(format!("Unexpected token {t:?}. Expected Number")),
                    };
                    Ok(Action::DeleteTask(idx))
                }
                Some(t) => Err(format!("Unexpected token {t:?}. Expected Task/List")),
                None => Err("Unexpected end of statement".to_string()),
            }
        }
        Some(&Token::Complete) => {
            match tokens.get(1) {
                Some(Token::Number(i)) => Ok(Action::Complete(*i as usize)),
                Some(t) => Err(format!("Unexpected token {t:?}. Expected Number")),
                None => Err("Unexpected end of statement".to_string()),
            }
        }
        Some(&Token::Help) => Ok(Action::Help),
        Some(&Token::Close) => Ok(Action::Close),
        Some(&Token::Exit) => Ok(Action::Exit),
        Some(&Token::Save) => Ok(Action::Save),
        Some(t) => Err(format!("Unexpected token {t:?}")),
        None => Ok(Action::None),
    }
}
fn stdin_to_action() -> Result<Action, Box<dyn std::error::Error>> {
    let mut target = String::new();
    std::io::stdin().read_line(&mut target)?;
    let tokens = lex(&target)?;
    let act = parse_target(&tokens)?;
    Ok(act)
}
