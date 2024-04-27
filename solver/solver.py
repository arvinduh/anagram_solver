import os
import config

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
    max_letters: int = config.DFLT_MAX_LETTERS,
    min_letters: int = config.MIN_LENGTH,
    dst_dir: str = config.SORTED_FILES,
) -> list[dict[str]] | None:
    """
    Solve the word puzzle.
    :param letters: The letters to use in the puzzle.
    :param dst_dir: The directory containing the word bank files.
    :return: The words that can be made from the letters.
    """
    letters = letters.lower()

    words = {}
    for i in range(
        min(max_letters, config.MAX_LENGTH),
        max(min_letters, config.MIN_LENGTH) - 1,
        -1
    ):
        src_fn = os.path.join(dst_dir, config.FN_FORMAT.format(letter_count=i))
        if not os.path.exists(src_fn):
            continue

        with open(src_fn, 'r') as src_file:
            src_words = src_file.readlines()
        
        src_words = [word.strip() for word in src_words]
        words[i] = [word for word in src_words if is_anagram(word, letters)]
    
    return words