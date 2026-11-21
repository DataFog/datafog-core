"""Synthetic API fixtures for feedback accounting, including invalid cohorts."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('feedback', Path(__file__).with_name('report-ci-feedback.py'))
feedback = importlib.util.module_from_spec(spec)
spec.loader.exec_module(feedback)


def fixture():
    return {'repository': 'example/repo', 'collected_at': '2026-11-20T00:00:00Z', 'subjects': [{
        'pr': 12, 'head_sha': 'current', 'current_head_sha': 'current', 'files': [{'filename': 'README.md'}],
        'runs': [{'id': 1, 'pr_identity_verified': True, 'name': 'Python binding', 'head_sha': 'current', 'run_attempt': 1,
                  'status': 'completed', 'conclusion': 'success', 'created_at': '2026-11-20T00:00:00Z',
                  'jobs': [{'id': 9, 'name': 'Python', 'html_url': 'https://example/job/9',
                            'head_sha': 'current', 'run_attempt': 1, 'status': 'completed', 'conclusion': 'success',
                            'created_at': '2026-11-20T00:00:02Z', 'started_at': '2026-11-20T00:02:02Z',
                            'completed_at': '2026-11-20T00:02:07Z'}]}]}]}


class FeedbackTests(unittest.TestCase):
    def row(self, data):
        return feedback.report(data)['batches'][0]

    def test_current_docs_wait_and_execution_are_distinct(self):
        row = self.row(fixture())
        self.assertTrue(row['eligible'])
        self.assertEqual(row['cohort'], 'docs')
        self.assertEqual(row['observed_feedback_seconds'], 127)
        self.assertEqual(row['execution_seconds_sum'], 5)
        self.assertEqual(row['jobs'][0]['wait_seconds'], 120)

    def test_failures_are_included_separately(self):
        data = fixture()
        data['subjects'][0]['runs'][0]['conclusion'] = 'failure'
        result = feedback.report(data)
        self.assertTrue(result['batches'][0]['eligible'])
        self.assertEqual(result['cohort_summaries']['docs']['failed_first_attempts'], 1)
        self.assertEqual(result['cohort_summaries']['docs']['success_only']['n'], 0)

    def test_empty_inventory_and_missing_timestamps(self):
        data = fixture()
        data['subjects'][0]['runs'][0]['jobs'][0]['started_at'] = None
        self.assertIn('incomplete_job_timing', self.row(data)['exclusions'])
        data['subjects'][0]['runs'] = []
        self.assertIn('no_technical_runs', self.row(data)['exclusions'])

    def test_ambiguous_pr_identity_excluded(self):
        data = fixture()
        data['subjects'][0]['runs'][0]['pr_identity_verified'] = False
        self.assertIn('unverified_pr_identity', self.row(data)['exclusions'])

    def test_percentiles_have_small_sample_guards(self):
        data = fixture()
        data['subjects'] = [copy.deepcopy(data['subjects'][0]) for _ in range(20)]
        result = feedback.report(data)['cohort_summaries']['docs']['completed_first_attempts']
        self.assertEqual(result, {'n': 20, 'p50_seconds': 127, 'p95_seconds': 127})
        data['subjects'] = data['subjects'][:4]
        self.assertIsNone(feedback.report(data)['cohort_summaries']['docs']['completed_first_attempts']['p50_seconds'])

    def test_stale_head_excluded(self):
        data = fixture()
        data['subjects'][0]['current_head_sha'] = 'new'
        self.assertIn('stale_head', self.row(data)['exclusions'])

    def test_code_and_renamed_code_are_not_prose(self):
        for file in [{'filename': 'docs/contracts/example.md'},
                     {'filename': 'README.md', 'previous_filename': 'crates/core/src/lib.rs'}]:
            data = fixture()
            data['subjects'][0]['files'] = [file]
            self.assertEqual(self.row(data)['cohort'], 'code')

    def test_queue_cohort(self):
        data = fixture()
        del data['subjects'][0]['pr']
        self.assertEqual(self.row(data)['cohort'], 'queue')

    def test_reruns_cancellations_and_incomplete_runs_excluded(self):
        for field, value, reason in [('run_attempt', 2, 'retried_run'),
                                      ('conclusion', 'cancelled', 'cancelled_or_other_run'),
                                      ('status', 'in_progress', 'incomplete_run')]:
            data = fixture()
            data['subjects'][0]['runs'][0][field] = value
            self.assertIn(reason, self.row(data)['exclusions'])

    def test_policy_metadata_does_not_extend_batch(self):
        data = fixture()
        policy = copy.deepcopy(data['subjects'][0]['runs'][0])
        policy.update(id=2, name='Changelog', created_at='2026-11-20T02:00:00Z')
        data['subjects'][0]['runs'].append(policy)
        self.assertEqual(self.row(data)['observed_feedback_seconds'], 127)

    def test_batches_and_duplicate_workflows_are_not_extra_samples(self):
        data = fixture()
        run = copy.deepcopy(data['subjects'][0]['runs'][0])
        run['id'] = 2
        data['subjects'][0]['runs'].append(run)
        self.assertIn('duplicate_workflow', self.row(data)['exclusions'])
        run['created_at'] = '2026-11-20T00:02:00Z'
        rows = feedback.report(data)['batches']
        self.assertIn('superseded_batch', rows[0]['exclusions'])
        self.assertEqual(len(rows), 2)

    def test_skipped_jobs_do_not_allocate_or_parse_invalid_timestamps(self):
        data = fixture()
        data['subjects'][0]['runs'][0]['jobs'].append({'conclusion': 'skipped'})
        self.assertEqual(self.row(data)['allocations_observed'], 1)

    def test_invalid_job_identity_or_time_excluded(self):
        data = fixture()
        job = data['subjects'][0]['runs'][0]['jobs'][0]
        job['head_sha'] = 'stale'
        job['started_at'] = '2026-11-20T00:03:00Z'
        row = self.row(data)
        self.assertIn('mixed_job_identity', row['exclusions'])
        self.assertIn('incomplete_job_timing', row['exclusions'])


if __name__ == '__main__':
    unittest.main()
