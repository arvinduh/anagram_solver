import os
from .solver import solve
import config


def clear_console() -> None:
    """
    Clear the console.
    :return: None
    """
    os.system('cls' if os.name == 'nt' else 'clear')


def prompt_user(
    min_letters: int = config.MIN_LENGTH,
    max_letters: int = config.DFLT_MAX_LETTERS,
    dst_dir: str = config.SORTED_FILES,
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
    
    words = solve(letters=user_in, min_letters=min_letters, max_letters=max_letters, dst_dir=dst_dir)

    longest = max(words.keys())
    shortest = min(words.keys())
    curr = longest
    user_in = ""
    while user_in not in ['e', 'exit']:
        clear_console()
        print(f'{'[=]':>6} {curr} letter anagrams:')
        for i, word in enumerate(words[curr]):
            num = f'[{i + 1}]'
            print(f'{num:>6} {word}')
        
        print(
            f'\n{'[=]':>6} Choose one of the following:',
            'Any key to see -1 letter anagrams',
            'Prev (p) to see +1 letter anagrams',
            'Exit (e) to exit',
            sep=f'\n{'[*]':>10} '
        )
        user_in = input(f'{'[+]':>6} ').lower()

        if user_in in ['p', 'prev'] and curr < longest:
            curr += 1
        elif user_in.isdigit() and int(user_in) in words.keys():
            curr = int(user_in)
        elif user_in not in ['e', 'exit'] and curr > shortest:
            curr -= 1