import unittest
import hashlib
from pathlib import Path
import tempfile
from quality_smoke import classify_render_error, artifact_hash, verify_artifact, verify_fallback_pixels


class ErrorClassificationTests(unittest.TestCase):
    def test_fallback_pixels_do_not_certify_notification(self):
        metadata=dict(width=1,height=1,pixel_format='argb8',channel_order='ARGB',origin=dict(x=0,y=0),rowbytes=4)
        raw=bytes([255,1,2,3])
        self.assertEqual(verify_fallback_pixels(metadata,metadata,raw,raw),
                         'blocked_error_notification_unverified')
        for key in metadata:
            altered=dict(metadata)
            del altered[key]
            with self.assertRaises(AssertionError):
                verify_fallback_pixels(metadata,altered,raw,raw)
        for output in [b'',bytes([255,1,2,4]),raw[:2]]:
            with self.assertRaises(AssertionError):
                verify_fallback_pixels(metadata,metadata,raw,output)

    def test_artifact_identity_is_streamed_and_changes_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'artifact.bin'
            original = b'abc' * 500000
            path.write_bytes(original)
            expected = artifact_hash(path)
            self.assertEqual(expected, hashlib.sha256(original).hexdigest())
            verify_artifact(path, expected)
            path.write_bytes(b'replacement')
            with self.assertRaisesRegex(RuntimeError, 'changed during run'):
                verify_artifact(path, expected)
            path.unlink()
            with self.assertRaises(FileNotFoundError):
                verify_artifact(path, expected)

    def test_expected_error_transport_is_not_a_pass(self):
        message='malformed_error_response: frame 0 error response carried output/resize fields'
        self.assertEqual(classify_render_error('expected-model-error',message),'blocked_host_error_transport')

    def test_other_errors_are_not_hidden(self):
        for message in ['timeout','plugin crash','malformed_error_response: different reason','']:
            self.assertEqual(classify_render_error('expected-model-error',message),'failed_unclassified')

    def test_normal_render_never_inherits_negative_oracle(self):
        message='malformed_error_response: error response carried output/resize fields'
        self.assertEqual(classify_render_error('passthrough',message),'failed_unclassified')


if __name__=='__main__':
    unittest.main()
