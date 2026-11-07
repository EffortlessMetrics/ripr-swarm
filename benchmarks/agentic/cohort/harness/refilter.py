import sys

import oracle


def main(task_ids=None):
    for task_id in sys.argv[1:] if task_ids is None else task_ids:
        references = oracle.completed_agent_tests(task_id, (0, 1))
        oracle.filter_mutants(task_id, references)


if __name__ == "__main__":
    main()
