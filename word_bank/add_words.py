"""
Add words from a file to the word bank.
"""
import os


def add_words(
    fn: str,
    dst_dir: str = './files/sorted/',
) -> bool:
    """
    Add words from a file to the word bank.
    :param fn: The file containing the words to add.
    :param dst_dir: The directory containing the word bank files.
    :return: True if the words were added successfully, False otherwise.
    """
    if not os.path.exists(fn):
        return False
    
    with open(fn, 'r') as file:
        new_words = file.readlines()
    
    new_words = [word.strip().lower() for word in new_words]
    
    for i in range(3, 8):
        src_fn = os.path.join(dst_dir, f'{i}_letter_words.txt')

        if not os.path.exists(src_fn):
            src_words = {word for word in new_words if len(word) == i}
        else:
            with open(src_fn, 'r') as src_file:
                src_words = src_file.readlines()
            
            src_words = [word.strip().lower() for word in src_words]
            src_words += [word for word in new_words if len(word) == i]
            src_words = set(src_words)

        with open(src_fn, 'w') as src_file:
            for word in src_words:
                src_file.write(word + '\n')
    
    return True


if __name__ == '__main__':
    add_words(
        fn='./files/original/Collins Scrabble Words (2019).txt',
        dst_dir='./files/sorted/'
    )