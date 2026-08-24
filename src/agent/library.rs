#[derive(Debug, Clone, Copy)]
pub struct Prompt {
    pub title: &'static str,
    pub category: &'static str,
    pub text: &'static str,
}

pub fn builtin_prompts() -> &'static [Prompt] {
    &[ 
        Prompt {
            title: "Sort Downloads",
            category: "Files",
            text: "Sort my Downloads folder by file type into subfolders. Don't touch hidden files.",
        },
        Prompt {
            title: "Find duplicate photos",
            category: "Files",
            text: "Find likely duplicate photos under this workspace by name and size, and list the groups.",
        },
        Prompt {
            title: "Collect invoices",
            category: "Files",
            text: "Find files whose names look like invoices and move them into a new Invoices folder on the Desktop.",
        },
        Prompt {
            title: "Clean Desktop",
            category: "Files",
            text: "Propose and then apply a tidy layout for this folder: documents, images, archives, and everything else.",
        },
        Prompt {
            title: "Summarize latest CSV",
            category: "Data",
            text: "Find the newest CSV in this workspace, summarize the columns and a few notable rows, and write the summary next to it.",
        },
        Prompt {
            title: "Chart a spreadsheet",
            category: "Data",
            text: "Open the newest spreadsheet or CSV here, describe the interesting numbers, and write a markdown report with a simple ASCII chart.",
        },
        Prompt {
            title: "Project status",
            category: "Code",
            text: "Inspect this workspace as a software project. Summarize the stack, how to run it, and the riskiest open issues you can see from the files.",
        },
        Prompt {
            title: "Fix compile errors",
            category: "Code",
            text: "Run the project's tests or build, then fix any errors you find with surgical edits.",
        },
        Prompt {
            title: "Write a README",
            category: "Code",
            text: "Write or refresh README.md for this project from the actual files, commands, and layout you find here.",
        },
        Prompt {
            title: "Convert images",
            category: "Files",
            text: "Convert image files in this folder to JPG when a converter is available locally. Keep originals.",
        },
    ]
}

pub fn system_prompt(workspace: &str, knowledge: &[String]) -> String {
    let mut prompt = format!(
        "You are Desktop Commander running as Commando on Linux.\n\
         You execute on the user's computer. Prefer doing the work over describing it.\n\n\
         Workspace: {workspace}\n\
         Expand ~ to the home directory. Use absolute paths in tool calls when you can.\n\
         Prefer surgical edits over rewriting whole files.\n\
         Use list_directory and search_files before guessing paths.\n\
         For conversions, sorting, git, and system tools, use run_command.\n\
         Do not run destructive commands against `/` or device files.\n\
         After you finish, write a short summary of what changed.\n\
         Current time: {}.\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
    );
    if !knowledge.is_empty() {
        prompt.push_str("\nAttached knowledge:\n");
        for item in knowledge {
            prompt.push_str(item);
            prompt.push('\n');
        }
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_nonempty() {
        assert!(!builtin_prompts().is_empty());
        assert!(system_prompt("~/Desktop", &[]).contains("~/Desktop"));
    }
}
