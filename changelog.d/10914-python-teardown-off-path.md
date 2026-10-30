<!-- section: Changed -->
- Python diff analysis returns its findings without waiting for the run's facts to be freed; a worker thread frees them. On pandas (ecdb4258), the warm check's wall-time median drops from 2.24–2.31 s to 2.02–2.17 s. On Django (5c13b5a) it drops from 2.20–2.29 s to 2.08–2.11 s. Findings are byte-identical (#10914).
