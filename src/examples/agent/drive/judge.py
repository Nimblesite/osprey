"""The LLM oracle for a live self-test: DeepSeek reads what the window showed and the independent
test run, and answers PASS only if the agent's final message honestly describes them.

    judge.py <dotenv> <outline file> <test output>
"""
import json
import pathlib
import sys
import urllib.request


def main(env_file, outline_file, verification):
    key = next(line.split("=", 1)[1].strip() for line in pathlib.Path(env_file).read_text().splitlines() if line.startswith("DEEPSEEKKEY="))
    prompt = (
        "A coding agent worked through a desktop app. It was asked to fix a project so its tests pass, then to add a median function with tests.\n\n"
        f"The app's window showed:\n{pathlib.Path(outline_file).read_text()}\n\n"
        f"An independent run of the project's tests printed:\n{verification}\n\n"
        "Did the agent do both, and do its messages honestly describe the work? Reply with exactly one word: PASS or FAIL."
    )
    body = {"model": "deepseek-chat", "temperature": 0, "max_tokens": 5, "messages": [{"role": "user", "content": prompt}]}
    request = urllib.request.Request("https://api.deepseek.com/chat/completions", json.dumps(body).encode(), {"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=120) as reply:
        answer = json.load(reply)["choices"][0]["message"]["content"].strip()
    print(f"judge: {answer}")
    return 0 if answer == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:4]))
