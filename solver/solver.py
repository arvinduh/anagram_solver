import os


def clear_console() -> None:
    """
    Clear the console.
    :return: None
    """
    os.system('cls' if os.name == 'nt' else 'clear')


def is_anagram(
    word: str,
    letters: str
) -> bool:
    """
    Check if a word is an anagram of the given letters.
    :param word: The word to check.
    :param letters: The letters to check against.
    :return: True if the word is an anagram of the letters, False otherwise.
    """
    let_list = list(letters.lower())
    for letter in word:
        if letter not in let_list:
            return False
        let_list.remove(letter)
    
    return True

def solve(
    letters: str,
    max_letters: int = 6,
    dst_dir: str = './files/sorted/',
) -> list[dict[str]] | None:
    """
    Solve the word puzzle.
    :param letters: The letters to use in the puzzle.
    :param dst_dir: The directory containing the word bank files.
    :return: The words that can be made from the letters.
    """
    if len(letters) != max_letters:
        return None
    letters = letters.lower()

    words = []
    for i in range(max_letters, 2, -1):
        src_fn = os.path.join(dst_dir, f'{i}_letter_words.txt')
        if not os.path.exists(src_fn):
            continue

        with open(src_fn, 'r') as src_file:
            src_words = src_file.readlines()
        
        src_words = [word.strip() for word in src_words]
        words.append([word for word in src_words if is_anagram(word, letters)])
    
    return words


def prompt_user(
    max_letters: int = 6,
    dst_dir: str = './files/sorted/',
) -> None:
    """
    Prompt the user for input and display the results.
    :param max_letters: The maximum number of letters to use.
    :param dst_dir: The directory containing the word bank files.
    :return: None
    """
    user_in = ""
    error = None
    while len(user_in) != max_letters or not user_in.isalpha():
        clear_console()
        print(f'[=] Enter {max_letters} letters: ')
        if error is not None:
            print(f'[-] {error}')
        user_in = input('[+] ').strip().lower()

        if len(user_in) != max_letters:
            error = f'Please enter {max_letters} letters.'
        elif not user_in.isalpha():
            error = 'Please enter only letters.'
    
    words = solve(user_in, max_letters, dst_dir)

    clear_console()
    for word_set in words:
        for i, word in enumerate(word_set):
            print(f'{f'[{i + 1}]':>6} {word}')
        input(f'{'[=]':>6} Press enter to continue')
        clear_console()